//! Bounded virtio-vsock streams backed by real host Unix sockets.
//! Acceptance is a transport event, never authenticated guest readiness.
use crate::vsock_wire::{self as wire, Header};
use relay_core::RelayError;
use std::{
    collections::{BTreeMap, VecDeque},
    io::{ErrorKind, Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::mpsc::{sync_channel, Receiver, SyncSender},
};

pub(crate) const HOST_CID: u64 = 2;
pub(crate) const GUEST_CID: u64 = 3;
const WINDOW: usize = wire::MAX_DATA;
const MAX_FLOWS: usize = 16;
const MAX_LISTENERS: usize = 8;
const MAX_PACKETS: usize = 256;
const MAX_QUEUED: usize = 1024 * 1024;
const CHUNK: usize = 4096;
const RECEIVE_CLOSED: u32 = 1;
const SEND_CLOSED: u32 = 2;

type Key = (u32, u32); // Guest port, host port.

/// A connected byte stream. Caller must authenticate the session separately.
pub struct VsockConnection {
    pub guest_port: u32,
    pub host_port: u32,
    pub stream: UnixStream,
}

#[derive(Default)]
struct PeerCredit {
    allocated: u32,
    transmitted: u32,
    forwarded: u32,
}
impl PeerCredit {
    fn update(&mut self, allocated: u32, forwarded: u32) -> bool {
        // Validate advancement independently of buf_alloc. Otherwise a forged
        // fwd_cnt and a large allocation can manufacture modular credit.
        if forwarded.wrapping_sub(self.forwarded) > self.transmitted.wrapping_sub(self.forwarded) {
            return false;
        }
        self.allocated = allocated;
        self.forwarded = forwarded;
        true
    }
    fn free(&self) -> u32 {
        wire::available_credit(self.allocated, self.transmitted, self.forwarded).unwrap_or(0)
    }
    fn reserve(&mut self, bytes: u32) -> bool {
        if bytes > self.free() {
            return false;
        }
        self.transmitted = self.transmitted.wrapping_add(bytes);
        true
    }
}

#[cfg(kani)]
#[kani::proof]
fn forwarding_cannot_acknowledge_unsent_bytes() {
    let mut credit = PeerCredit {
        allocated: kani::any(),
        transmitted: kani::any(),
        forwarded: kani::any(),
    };
    let before = (credit.allocated, credit.transmitted, credit.forwarded);
    let allocated: u32 = kani::any();
    let forwarded: u32 = kani::any();
    let advance = forwarded.wrapping_sub(before.2);
    let outstanding = before.1.wrapping_sub(before.2);
    assert_eq!(credit.update(allocated, forwarded), advance <= outstanding);
    if advance > outstanding {
        assert_eq!(
            (credit.allocated, credit.transmitted, credit.forwarded),
            before
        );
    } else {
        assert_eq!(
            credit.transmitted.wrapping_sub(credit.forwarded),
            outstanding - advance
        );
    }
}

struct Flow {
    socket: UnixStream,
    peer: PeerCredit,
    pending: VecDeque<u8>,
    forwarded: u32,
    shutdown: u32,
    host_eof: bool,
    eof_announced: bool,
    write_closed: bool,
    credit_dirty: bool,
}
impl Flow {
    fn header(&self, key: Key, op: u16, len: usize, flags: u32) -> Header {
        Header {
            src_cid: HOST_CID,
            dst_cid: GUEST_CID,
            src_port: key.1,
            dst_port: key.0,
            len: len as u32,
            kind: wire::STREAM,
            op,
            flags,
            buf_alloc: WINDOW as u32,
            fwd_cnt: self.forwarded,
        }
    }
}

#[derive(Default)]
pub(crate) struct Vsock {
    #[cfg(feature = "kernel-probe")]
    traced_packets: usize,
    listeners: BTreeMap<u32, SyncSender<VsockConnection>>,
    flows: BTreeMap<Key, Flow>,
    outgoing: VecDeque<Vec<u8>>,
    queued: usize,
}
impl Vsock {
    pub(crate) fn listen(&mut self, port: u32) -> Result<Receiver<VsockConnection>, RelayError> {
        if port == 0 || self.listeners.contains_key(&port) || self.listeners.len() >= MAX_LISTENERS
        {
            return Err(RelayError::Failed("vsock listener unavailable".into()));
        }
        let (sender, receiver) = sync_channel(MAX_FLOWS);
        self.listeners.insert(port, sender);
        Ok(receiver)
    }
    pub(crate) fn reset(&mut self) {
        self.flows.clear();
        self.outgoing.clear();
        self.queued = 0;
    }
    pub(crate) fn can_accept_packet(&self) -> bool {
        self.has_room(wire::HEADER_BYTES + wire::MAX_DATA)
    }
    fn has_room(&self, bytes: usize) -> bool {
        self.outgoing.len() < MAX_PACKETS && bytes <= MAX_QUEUED - self.queued
    }
    fn enqueue(&mut self, header: Header, data: &[u8]) -> Result<(), RelayError> {
        let size = wire::HEADER_BYTES + data.len();
        if !self.has_room(size) {
            return Err(RelayError::Failed("vsock outgoing budget exhausted".into()));
        }
        let packet = header.encode(data)?;
        self.queued += packet.len();
        self.outgoing.push_back(packet);
        Ok(())
    }
    pub(crate) fn front(&self) -> Option<&[u8]> {
        self.outgoing.front().map(Vec::as_slice)
    }
    pub(crate) fn pop(&mut self) {
        if let Some(packet) = self.outgoing.pop_front() {
            self.queued -= packet.len();
        }
    }
    fn discard_flow_packets(&mut self, key: Key) {
        self.outgoing.retain(|packet| {
            let (header, _) = Header::decode(packet).expect("host-created vsock packet");
            (header.dst_port, header.src_port) != key
        });
        self.queued = self.outgoing.iter().map(Vec::len).sum();
    }
    fn reject(&mut self, header: Header) -> Result<(), RelayError> {
        let key = (header.src_port, header.dst_port);
        self.flows.remove(&key);
        self.discard_flow_packets(key);
        if header.op == wire::RESET {
            return Ok(());
        }
        self.enqueue(
            Header {
                src_cid: HOST_CID,
                dst_cid: GUEST_CID,
                src_port: header.dst_port,
                dst_port: header.src_port,
                len: 0,
                kind: header.kind,
                op: wire::RESET,
                flags: 0,
                buf_alloc: 0,
                fwd_cnt: 0,
            },
            &[],
        )
    }
    pub(crate) fn receive(&mut self, packet: &[u8]) -> Result<(), RelayError> {
        let (header, data) = Header::decode(packet)?;
        #[cfg(feature = "kernel-probe")]
        if self.traced_packets < 128 {
            eprintln!("Relay vsock guest header: {header:?}");
            self.traced_packets += 1;
        }
        // Do not let a packet from another CID tear down an existing flow.
        if header.src_cid != GUEST_CID || header.dst_cid != HOST_CID {
            return Ok(());
        }
        if header.kind != wire::STREAM
            || (header.op != wire::DATA && !data.is_empty())
            || (header.op != wire::SHUTDOWN && header.flags != 0)
        {
            return self.reject(header);
        }
        let key = (header.src_port, header.dst_port);
        if header.op == wire::REQUEST {
            if self.flows.contains_key(&key)
                || self.flows.len() >= MAX_FLOWS
                || header.fwd_cnt != 0
                || header.src_port == 0
            {
                return self.reject(header);
            }
            let Some(listener) = self.listeners.get(&header.dst_port) else {
                return self.reject(header);
            };
            let (socket, stream) =
                UnixStream::pair().map_err(|e| RelayError::Failed(e.to_string()))?;
            socket
                .set_nonblocking(true)
                .map_err(|e| RelayError::Failed(e.to_string()))?;
            let connection = VsockConnection {
                guest_port: key.0,
                host_port: key.1,
                stream,
            };
            if listener.try_send(connection).is_err() {
                return self.reject(header);
            }
            let flow = Flow {
                socket,
                peer: PeerCredit {
                    allocated: header.buf_alloc,
                    ..PeerCredit::default()
                },
                pending: VecDeque::new(),
                forwarded: 0,
                shutdown: 0,
                host_eof: false,
                eof_announced: false,
                write_closed: false,
                credit_dirty: false,
            };
            self.enqueue(flow.header(key, wire::RESPONSE, 0, 0), &[])?;
            self.flows.insert(key, flow);
            return Ok(());
        }
        let Some(flow) = self.flows.get_mut(&key) else {
            return self.reject(header);
        };
        if header.op == wire::RESET {
            self.flows.remove(&key);
            self.discard_flow_packets(key);
            return Ok(());
        }
        if !flow.peer.update(header.buf_alloc, header.fwd_cnt) {
            return self.reject(header);
        }
        match header.op {
            wire::DATA
                if flow.shutdown & SEND_CLOSED == 0
                    && data.len() <= WINDOW - flow.pending.len() =>
            {
                flow.pending.extend(data);
            }
            wire::CREDIT_UPDATE => {}
            wire::CREDIT_REQUEST => {
                flow.credit_dirty = true;
            }
            wire::SHUTDOWN if header.flags != 0 && header.flags & !3 == 0 => {
                flow.shutdown |= header.flags;
            }
            _ => return self.reject(header),
        }
        Ok(())
    }

    /// Poll real sockets without blocking the CPU or consuming bytes beyond
    /// peer credit. Queued payloads reserve credit before guest RX delivery.
    pub(crate) fn poll(&mut self) -> Result<(), RelayError> {
        let keys: Vec<_> = self.flows.keys().copied().collect(); // At most 16.
        for key in keys {
            if !self.has_room(wire::HEADER_BYTES + CHUNK) {
                break;
            }
            let flow = self.flows.get_mut(&key).unwrap();
            let mut broken = false;
            if !flow.pending.is_empty() {
                let (first, _) = flow.pending.as_slices();
                match flow.socket.write(first) {
                    Ok(0) => broken = true,
                    Ok(n) => {
                        flow.pending.drain(..n);
                        flow.forwarded = flow.forwarded.wrapping_add(n as u32);
                        flow.credit_dirty = true;
                    }
                    Err(e)
                        if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                    Err(_) => broken = true,
                }
            }
            if flow.shutdown & SEND_CLOSED != 0 && flow.pending.is_empty() && !flow.write_closed {
                broken |= flow.socket.shutdown(Shutdown::Write).is_err();
                flow.write_closed = true;
            }
            if broken || (flow.shutdown == 3 && flow.pending.is_empty()) {
                let header = flow.header(key, wire::RESET, 0, 0);
                self.flows.remove(&key);
                self.discard_flow_packets(key);
                self.enqueue(header, &[])?;
                continue;
            }
            if flow.credit_dirty {
                let header = flow.header(key, wire::CREDIT_UPDATE, 0, 0);
                flow.credit_dirty = false;
                self.enqueue(header, &[])?;
                continue;
            }
            if flow.host_eof && !flow.eof_announced {
                let header = flow.header(key, wire::SHUTDOWN, 0, SEND_CLOSED);
                flow.eof_announced = true;
                self.enqueue(header, &[])?;
                continue;
            }
            if flow.host_eof || flow.shutdown & RECEIVE_CLOSED != 0 {
                continue;
            }
            let mut buffer = [0u8; CHUNK];
            let allowed = CHUNK.min(flow.peer.free() as usize);
            if allowed == 0 {
                continue;
            }
            match flow.socket.read(&mut buffer[..allowed]) {
                Ok(0) => {
                    flow.host_eof = true;
                }
                Ok(n) => {
                    assert!(flow.peer.reserve(n as u32));
                    let header = flow.header(key, wire::DATA, n, 0);
                    self.enqueue(header, &buffer[..n])?;
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                Err(_) => {
                    let header = flow.header(key, wire::RESET, 0, 0);
                    self.flows.remove(&key);
                    self.discard_flow_packets(key);
                    self.enqueue(header, &[])?;
                }
            }
        }
        Ok(())
    }
}

/// Development-only packet-sequence harness, excluded from product builds.
#[cfg(feature = "fuzzing")]
pub(crate) fn fuzz_packets(mut input: &[u8]) {
    let mut device = Vsock::default();
    let _listener = device.listen(1024).unwrap();
    for _ in 0..512 {
        if input.len() < 2 {
            break;
        }
        let length = u16::from_le_bytes(input[..2].try_into().unwrap()) as usize;
        input = &input[2..];
        if length > input.len() {
            break;
        }
        if device.can_accept_packet() {
            let _ = device.receive(&input[..length]);
        }
        assert!(device.flows.len() <= MAX_FLOWS);
        assert!(device.queued <= MAX_QUEUED);
        assert!(device.outgoing.len() <= MAX_PACKETS);
        assert!(device
            .flows
            .values()
            .all(|flow| flow.pending.len() <= WINDOW));
        input = &input[length..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn guest(op: u16, data: &[u8], fwd_cnt: u32, flags: u32) -> Vec<u8> {
        Header {
            src_cid: GUEST_CID,
            dst_cid: HOST_CID,
            src_port: 5000,
            dst_port: 1024,
            len: data.len() as u32,
            kind: wire::STREAM,
            op,
            flags,
            buf_alloc: 8,
            fwd_cnt,
        }
        .encode(data)
        .unwrap()
    }
    fn take(device: &mut Vsock) -> (Header, Vec<u8>) {
        let (header, data) = Header::decode(device.front().unwrap()).unwrap();
        let data = data.to_vec();
        device.pop();
        (header, data)
    }
    #[test]
    #[cfg(not(miri))]
    fn real_host_stream_credit_and_half_close() {
        let mut device = Vsock::default();
        let listener = device.listen(1024).unwrap();
        device.receive(&guest(wire::REQUEST, &[], 0, 0)).unwrap();
        assert_eq!(take(&mut device).0.op, wire::RESPONSE);
        let mut host = listener.try_recv().unwrap().stream;
        host.set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        device.receive(&guest(wire::DATA, b"guest", 0, 0)).unwrap();
        device.poll().unwrap();
        let mut bytes = [0; 5];
        host.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"guest");
        assert_eq!(take(&mut device).0.fwd_cnt, 5);
        host.write_all(b"abcdefghijk").unwrap();
        device.poll().unwrap();
        assert_eq!(take(&mut device).1, b"abcdefgh");
        device.poll().unwrap();
        assert!(device.front().is_none());
        device
            .receive(&guest(wire::CREDIT_UPDATE, &[], 8, 0))
            .unwrap();
        device.poll().unwrap();
        assert_eq!(take(&mut device).1, b"ijk");
        device
            .receive(&guest(wire::SHUTDOWN, &[], 8, SEND_CLOSED))
            .unwrap();
        device.poll().unwrap();
        assert_eq!(host.read(&mut bytes).unwrap(), 0);
        host.shutdown(Shutdown::Write).unwrap();
        device.poll().unwrap();
        device.poll().unwrap();
        assert_eq!(take(&mut device).0.op, wire::SHUTDOWN);
        device
            .receive(&guest(wire::SHUTDOWN, &[], 11, RECEIVE_CLOSED))
            .unwrap();
        device.poll().unwrap();
        assert_eq!(take(&mut device).0.op, wire::RESET);
        assert!(device.flows.is_empty());
    }
    #[test]
    #[cfg(not(miri))]
    fn absent_listener_forged_credit_and_reset_fail_closed() {
        let mut device = Vsock::default();
        device.receive(&guest(wire::REQUEST, &[], 0, 0)).unwrap();
        assert_eq!(take(&mut device).0.op, wire::RESET);
        let listener = device.listen(1024).unwrap();
        device.receive(&guest(wire::REQUEST, &[], 0, 0)).unwrap();
        let _host = listener.try_recv().unwrap();
        take(&mut device);
        let mut forged = guest(wire::CREDIT_UPDATE, &[], 1, 0);
        forged[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
        device.receive(&forged).unwrap();
        assert_eq!(take(&mut device).0.op, wire::RESET);
        assert!(device.flows.is_empty());
        device.receive(&guest(wire::RESET, &[], 0, 0)).unwrap();
        assert!(device.front().is_none());
        device.receive(&guest(wire::REQUEST, &[], 0, 0)).unwrap();
        device.reset();
        assert!(device.flows.is_empty());
        assert_eq!(device.queued, 0);
    }
    #[test]
    fn credit_wrap_shrink_and_malicious_large_window() {
        let mut c = PeerCredit {
            allocated: 8,
            transmitted: 3,
            forwarded: u32::MAX - 4,
        };
        assert_eq!(c.free(), 0);
        assert!(c.update(2, 0));
        assert_eq!(c.free(), 0);
        assert!(c.update(8, 3));
        assert!(c.reserve(8));
        assert_eq!(c.free(), 0);
        assert!(!c.update(u32::MAX, 12));
        assert_eq!(c.forwarded, 3);
        assert!(c.update(8, 11));
        assert_eq!(c.free(), 8);
    }
    #[test]
    #[cfg(not(miri))]
    fn stream_limit_reset_and_pending_budget() {
        let mut device = Vsock::default();
        let listener = device.listen(1024).unwrap();
        let mut hosts = Vec::new();
        for n in 0..MAX_FLOWS {
            let mut packet = guest(wire::REQUEST, &[], 0, 0);
            packet[16..20].copy_from_slice(&(5000 + n as u32).to_le_bytes());
            device.receive(&packet).unwrap();
            let stream = listener.try_recv().unwrap().stream;
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                .unwrap();
            hosts.push(stream);
            assert_eq!(take(&mut device).0.op, wire::RESPONSE);
        }
        let mut overflow = guest(wire::REQUEST, &[], 0, 0);
        overflow[16..20].copy_from_slice(&9000u32.to_le_bytes());
        device.receive(&overflow).unwrap();
        assert_eq!(take(&mut device).0.op, wire::RESET);
        assert_eq!(device.flows.len(), MAX_FLOWS);
        device
            .receive(&guest(wire::DATA, &vec![7; WINDOW], 0, 0))
            .unwrap();
        assert_eq!(device.flows[&(5000, 1024)].pending.len(), WINDOW);
        device
            .receive(&guest(wire::DATA, b"overflow", 0, 0))
            .unwrap();
        assert_eq!(take(&mut device).0.op, wire::RESET);
        assert!(!device.flows.contains_key(&(5000, 1024)));
        device.reset();
        for mut host in hosts {
            assert_eq!(host.read(&mut [0]).unwrap(), 0);
        }
    }

    #[test]
    fn malformed_headers_never_allocate_flows_or_unbounded_packets() {
        let mut device = Vsock::default();
        let mut state = 0x31d53f218e542991u64;
        for n in 0..4096 {
            let mut packet = guest(wire::REQUEST, &[], 0, 0);
            for byte in &mut packet {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            packet.truncate(n % (wire::HEADER_BYTES + 1));
            let _ = device.receive(&packet);
            assert!(device.flows.is_empty());
            assert!(device.queued <= MAX_QUEUED);
            while device.front().is_some() {
                device.pop();
            }
        }
    }

    #[test]
    fn listener_and_outgoing_budgets_are_total_not_per_packet() {
        let mut device = Vsock::default();
        for p in 1..=MAX_LISTENERS as u32 {
            device.listen(p).unwrap();
        }
        assert!(device.listen(99).is_err());
        for n in 0..MAX_PACKETS {
            assert!(device.can_accept_packet());
            let mut packet = guest(wire::REQUEST, &[], 0, 0);
            packet[16..20].copy_from_slice(&(6000 + n as u32).to_le_bytes());
            device.receive(&packet).unwrap();
        }
        assert!(device.queued <= MAX_QUEUED);
        assert_eq!(device.outgoing.len(), MAX_PACKETS);
        assert!(!device.can_accept_packet());
    }
}

//! Host sockets behind the guest's 10.0.2.0/24 network.
//!
//! The StaticCpu thread only enqueues Ethernet frames. A worker owns the
//! file descriptors so a connect to cache.nixos.org cannot stall the guest.

use crate::net_frame::{
    forward_header, tcp_ethernet, udp_ethernet, udp_payload, ForwardHeader, TcpView, DNS_IP,
    UPSTREAM_DNS,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};

/// Host-side NAT counters. Heartbeats print these so a live StaticCpu smoke
/// can show SYN seen vs SYN-ACK queued without guest cooperation.
static NET_SYN: AtomicU64 = AtomicU64::new(0);
static NET_CONNECT_OK: AtomicU64 = AtomicU64::new(0);
static NET_CONNECT_FAIL: AtomicU64 = AtomicU64::new(0);
static NET_SYNACK_OK: AtomicU64 = AtomicU64::new(0);
static NET_SYNACK_DROP: AtomicU64 = AtomicU64::new(0);
static NET_RST: AtomicU64 = AtomicU64::new(0);
static NET_HOST_TX: AtomicU64 = AtomicU64::new(0);
static NET_HOST_RX: AtomicU64 = AtomicU64::new(0);

pub fn nat_counters() -> (u64, u64, u64, u64, u64, u64) {
    (
        NET_SYN.load(Ordering::Relaxed),
        NET_CONNECT_OK.load(Ordering::Relaxed),
        NET_CONNECT_FAIL.load(Ordering::Relaxed),
        NET_SYNACK_OK.load(Ordering::Relaxed),
        NET_SYNACK_DROP.load(Ordering::Relaxed),
        NET_RST.load(Ordering::Relaxed),
    )
}

/// Bytes written to and read from host TCP sockets. A live smoke with
/// `tx` ahead of `rx` accepted the guest ClientHello and never saw the
/// server flight.
pub fn nat_io_counters() -> (u64, u64) {
    (
        NET_HOST_TX.load(Ordering::Relaxed),
        NET_HOST_RX.load(Ordering::Relaxed),
    )
}

struct OutQueues {
    prio: SyncSender<Vec<u8>>,
    data: SyncSender<Vec<u8>>,
}

impl OutQueues {
    fn send_prio(&self, frame: Vec<u8>) -> bool {
        self.prio.try_send(frame).is_ok()
    }

    fn send_data(&self, frame: Vec<u8>) -> bool {
        self.data.try_send(frame).is_ok()
    }
}
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

// Long HTTPS downloads (nixexprs, substituters) open many short TCP flows.
// 32 filled during a stalled channels.nixos.org transfer and new SYNs were
// dropped, so later DNS/TCP retries failed even though UDP DNS is synthetic.
const MAX_FLOWS: usize = 128;
const TCP_FIN: u8 = 0x01;
const TCP_SYN: u8 = 0x02;
const TCP_RST: u8 = 0x04;
const TCP_PSH: u8 = 0x08;
const TCP_ACK: u8 = 0x10;
const POLLIN: i16 = 1;
const POLLOUT: i16 = 4;
const POLLERR: i16 = 8;
const POLLHUP: i16 = 16;

/// FIN only after every host byte has been ACKed (empty inflight and
/// nothing still queued toward the guest). Premature FIN truncates TLS.
pub(crate) fn tcp_may_send_fin(
    peer_fin: bool,
    sent_fin: bool,
    inflight_empty: bool,
    queued_empty: bool,
    open: bool,
) -> bool {
    peer_fin && !sent_fin && inflight_empty && queued_empty && open
}

#[cfg(kani)]
#[kani::proof]
fn fin_requires_empty_inflight() {
    let peer_fin: bool = kani::any();
    let sent_fin: bool = kani::any();
    let inflight_empty: bool = kani::any();
    let queued_empty: bool = kani::any();
    let open: bool = kani::any();
    let send = tcp_may_send_fin(peer_fin, sent_fin, inflight_empty, queued_empty, open);
    assert!(send == (peer_fin && !sent_fin && inflight_empty && queued_empty && open));
    if send {
        assert!(inflight_empty);
        assert!(queued_empty);
        assert!(!sent_fin);
    }
}

pub(crate) struct Worker {
    to_worker: Option<Sender<Vec<u8>>>,
    from_prio: Receiver<Vec<u8>>,
    from_data: Receiver<Vec<u8>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Worker {
    pub(crate) fn spawn() -> Self {
        // Guest TX must not drop SYN/DNS under load (auth24/25: curl connect
        // timeout with ~0 TCP ingress after dns ok). Unbounded send never
        // blocks the StaticCpu thread the way a full sync_channel drop did.
        let (to_worker, incoming) = mpsc::channel();
        // DNS replies ride a priority queue so TCP retransmit backlog cannot
        // starve resolv after a slow HTTPS transfer (auth21-23 failure mode).
        let (prio_tx, from_prio) = mpsc::sync_channel(256);
        let (data_tx, from_data) = mpsc::sync_channel(4096);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);
        let join = thread::spawn(move || run(incoming, prio_tx, data_tx, stop_worker));
        Self {
            to_worker: Some(to_worker),
            from_prio,
            from_data,
            stop,
            join: Some(join),
        }
    }

    pub(crate) fn submit(&self, frame: Vec<u8>) {
        if let Some(sender) = &self.to_worker {
            let _ = sender.send(frame);
        }
    }

    pub(crate) fn drain(&self, out: &mut VecDeque<Vec<u8>>) {
        // Keep the virtio RX staging queue short. auth24 showed ~124B guest
        // ingress (one DNS reply) while SYN-ACK sat behind a bloated deque.
        const CAP: usize = 128;
        let mut prio = Vec::new();
        while prio.len() < CAP {
            match self.from_prio.try_recv() {
                Ok(frame) => prio.push(frame),
                Err(_) => break,
            }
        }
        // Oldest priority first at the head.
        for frame in prio.into_iter().rev() {
            if out.len() >= CAP {
                let _ = out.pop_back();
            }
            out.push_front(frame);
        }
        while out.len() < CAP {
            match self.from_data.try_recv() {
                Ok(frame) => out.push_back(frame),
                Err(_) => break,
            }
        }
        // If the staging queue is still full, drop data to admit DNS/ICMP.
        loop {
            if out.len() < CAP {
                match self.from_prio.try_recv() {
                    Ok(frame) => out.push_front(frame),
                    Err(_) => break,
                }
            } else {
                match self.from_prio.try_recv() {
                    Ok(frame) => {
                        let _ = out.pop_back();
                        out.push_front(frame);
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.to_worker.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct TcpFlow {
    fd: i32,
    guest_mac: [u8; 6],
    guest_ip: [u8; 4],
    guest_port: u16,
    dest_ip: [u8; 4],
    dest_port: u16,
    our_isn: u32,
    guest_next: u32,
    our_next: u32,
    una: u32,
    guest_window: u32,
    window_scale: u8,
    mss: u16,
    inflight: Vec<u8>,
    queued: usize,
    dup_acks: u8,
    idle_polls: u16,
    peer_fin: bool,
    sent_fin: bool,
    /// SYN-ACK must reach the guest. try_send on a full data queue dropped it
    /// on auth24-26 (DNS ok, curl 28, ~0 TCP ingress). Control uses prio and
    /// retries until the frame is queued.
    syn_acked: bool,
    /// Guest ACKed our ISN+1. Until then keep re-injecting SYN-ACK: auth27
    /// queued synack:1 on the host while curl still hit connect timeout, so
    /// the virtio RX path can lose the first control frame.
    handshake_done: bool,
    synack_retries: u16,
    pending_out: Vec<u8>,
    phase: Phase,
}

struct UdpFlow {
    fd: i32,
    guest_mac: [u8; 6],
    guest_ip: [u8; 4],
    guest_port: u16,
    reply_ip: [u8; 4],
    reply_port: u16,
    real_ip: [u8; 4],
    real_port: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// SYN-ACK advertised to the guest; host connect is deferred. StaticCpu
    /// can take minutes of wall time between SYN-ACK and ClientHello; an
    /// early host connect makes CDNs idle-timeout (auth31: handshake_done +
    /// ClientHello seen, curl still (28)).
    SynReceived,
    Connecting,
    Open,
    Closing,
}

fn run(
    incoming: Receiver<Vec<u8>>,
    prio: SyncSender<Vec<u8>>,
    data: SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
) {
    let mut tcp = Vec::<TcpFlow>::new();
    let mut udp = Vec::<UdpFlow>::new();
    let mut isn = 0x1000_0000u32;
    let out = OutQueues { prio, data };
    while !stop.load(Ordering::Relaxed) {
        while let Ok(frame) = incoming.try_recv() {
            on_frame(&mut tcp, &mut udp, &out, &mut isn, &frame);
        }
        poll_io(&mut tcp, &mut udp, &out);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match incoming.recv_timeout(Duration::from_millis(2)) {
            Ok(frame) => on_frame(&mut tcp, &mut udp, &out, &mut isn, &frame),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    for flow in tcp.drain(..) {
        if flow.fd >= 0 {
            close_fd(flow.fd);
        }
    }
    for flow in udp.drain(..) {
        close_fd(flow.fd);
    }
}

fn on_frame(
    tcp: &mut Vec<TcpFlow>,
    udp: &mut Vec<UdpFlow>,
    outgoing: &OutQueues,
    isn: &mut u32,
    frame: &[u8],
) {
    let Some((header, view)) = forward_header(frame) else {
        return;
    };
    if header.proto == 6 {
        on_tcp(tcp, outgoing, isn, &header, &view);
    } else if header.proto == 17 {
        on_udp(udp, outgoing, &header, frame);
    }
}

fn on_tcp(
    tcp: &mut Vec<TcpFlow>,
    outgoing: &OutQueues,
    isn: &mut u32,
    header: &ForwardHeader,
    view: &TcpView<'_>,
) {
    eprintln!(
        "relay-net: tcp :{} -> {}.{}.{}.{}:{} flags={:02x} seq={} ack={} len={}",
        header.src_port,
        header.dst_ip[0],
        header.dst_ip[1],
        header.dst_ip[2],
        header.dst_ip[3],
        header.dst_port,
        view.flags,
        view.seq,
        view.ack,
        view.payload.len()
    );
    if (view.flags & TCP_RST) != 0 {
        if let Some(index) = find_tcp(tcp, header) {
            eprintln!(
                "relay-net: RST :{} phase={} handshake={}",
                tcp[index].guest_port,
                match tcp[index].phase {
                    Phase::SynReceived => "syn-received",
                    Phase::Connecting => "connecting",
                    Phase::Open => "open",
                    Phase::Closing => "closing",
                },
                tcp[index].handshake_done
            );
            if tcp[index].fd >= 0 {
                close_fd(tcp[index].fd);
            }
            tcp.swap_remove(index);
        }
        return;
    }
    if (view.flags & TCP_SYN) != 0 {
        NET_SYN.fetch_add(1, Ordering::Relaxed);
        if let Some(index) = tcp.iter().position(|flow| same_tcp(flow, header)) {
            // Retransmitted SYN before the guest ACK: refresh SYN-ACK only.
            if matches!(
                tcp[index].phase,
                Phase::SynReceived | Phase::Connecting | Phase::Open
            ) {
                let _ = send_syn_ack(outgoing, &mut tcp[index]);
            }
            return;
        }
        if tcp.len() >= MAX_FLOWS {
            reclaim_tcp_slot(tcp);
        }
        if tcp.len() >= MAX_FLOWS {
            return;
        }
        *isn = isn.wrapping_add(64_000);
        eprintln!(
            "relay-net: SYN :{} -> {}.{}.{}.{}:{}",
            header.src_port,
            header.dst_ip[0],
            header.dst_ip[1],
            header.dst_ip[2],
            header.dst_ip[3],
            header.dst_port
        );
        let mut flow = TcpFlow {
            fd: -1,
            guest_mac: header.guest_mac,
            guest_ip: header.guest_ip,
            guest_port: header.src_port,
            dest_ip: header.dst_ip,
            dest_port: header.dst_port,
            our_isn: *isn,
            guest_next: view.seq.wrapping_add(1),
            our_next: isn.wrapping_add(1),
            una: isn.wrapping_add(1),
            // SYN window is never scaled (RFC 1323).
            guest_window: u32::from(view.window),
            window_scale: view.window_scale,
            mss: view.mss,
            inflight: Vec::new(),
            queued: 0,
            dup_acks: 0,
            idle_polls: 0,
            peer_fin: false,
            sent_fin: false,
            syn_acked: false,
            handshake_done: false,
            synack_retries: 0,
            pending_out: Vec::new(),
            phase: Phase::SynReceived,
        };
        let _ = send_syn_ack(outgoing, &mut flow);
        tcp.push(flow);
        return;
    }
    let Some(index) = find_tcp(tcp, header) else {
        return;
    };
    note_ack(&mut tcp[index], view);
    if matches!(tcp[index].phase, Phase::SynReceived | Phase::Connecting | Phase::Open)
        && (view.flags & TCP_ACK) != 0
        && view.ack == tcp[index].our_isn.wrapping_add(1)
    {
        if !tcp[index].handshake_done {
            eprintln!(
                "relay-net: handshake_done :{} ack={}",
                tcp[index].guest_port,
                view.ack
            );
        }
        tcp[index].handshake_done = true;
        if tcp[index].phase == Phase::SynReceived {
            match tcp_open(tcp[index].dest_ip, tcp[index].dest_port) {
                Ok(fd) => {
                    tcp[index].fd = fd;
                    tcp[index].phase = Phase::Connecting;
                    eprintln!(
                        "relay-net: host_connect :{} -> {}.{}.{}.{}:{}",
                        tcp[index].guest_port,
                        tcp[index].dest_ip[0],
                        tcp[index].dest_ip[1],
                        tcp[index].dest_ip[2],
                        tcp[index].dest_ip[3],
                        tcp[index].dest_port
                    );
                }
                Err(()) => {
                    NET_CONNECT_FAIL.fetch_add(1, Ordering::Relaxed);
                    let _ = send_rst(outgoing, &tcp[index]);
                    tcp.swap_remove(index);
                    return;
                }
            }
        }
    }
    if !view.payload.is_empty() && view.seq == tcp[index].guest_next {
        let room = 65_536 - tcp[index].pending_out.len();
        if view.payload.len() <= room {
            let flow = &mut tcp[index];
            flow.pending_out.extend_from_slice(view.payload);
            flow.guest_next = flow.guest_next.wrapping_add(view.payload.len() as u32);
            // ACK even while the host connect is still pending. Waiting for
            // Phase::Open dropped the ACK, the guest retransmit no longer
            // matched guest_next, and curl sat until (28).
            let our_next = flow.our_next;
            send_tcp(outgoing, flow, our_next, TCP_ACK, &[]);
        }
    }
    let fin_seq = view.seq.wrapping_add(view.payload.len() as u32);
    if (view.flags & TCP_FIN) != 0 && fin_seq == tcp[index].guest_next {
        let flow = &mut tcp[index];
        flow.guest_next = flow.guest_next.wrapping_add(1);
        flow.phase = Phase::Closing;
        if flow.fd >= 0 {
            shutdown_write(flow.fd);
        }
        let our_next = flow.our_next;
        send_tcp(outgoing, flow, our_next, TCP_ACK, &[]);
    }
}

fn on_udp(
    udp: &mut Vec<UdpFlow>,
    outgoing: &OutQueues,
    header: &ForwardHeader,
    frame: &[u8],
) {
    let Some(payload) = udp_payload(frame) else {
        return;
    };
    if payload.len() > 1400 {
        return;
    }
    if header.dst_ip == DNS_IP && header.dst_port == 53 {
        if let Some(name) = crate::net_frame::dns_qname(payload) {
            // getaddrinfo blocks. Doing it on this thread froze TCP reads
            // for the whole lookup, long enough for the CDN to drop the
            // unread ServerHello (auth31: ClientHello logged, curl (28)).
            let query = payload.to_vec();
            let guest_mac = header.guest_mac;
            let guest_ip = header.guest_ip;
            let src_port = header.src_port;
            let prio = outgoing.prio.clone();
            thread::spawn(move || {
                let addrs = std::net::ToSocketAddrs::to_socket_addrs(&(name.as_str(), 0u16))
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|addr| match addr {
                        std::net::SocketAddr::V4(v4) => Some(v4.ip().octets()),
                        std::net::SocketAddr::V6(_) => None,
                    })
                    .take(8)
                    .collect::<Vec<_>>();
                if let Some(answer) = crate::net_frame::dns_a_reply(&query, &addrs) {
                    if let Some(frame) =
                        udp_ethernet(guest_mac, guest_ip, src_port, DNS_IP, 53, &answer)
                    {
                        let _ = prio.try_send(frame);
                    }
                }
            });
            return;
        }
    }
    let real_ip = if header.dst_ip == DNS_IP {
        UPSTREAM_DNS
    } else {
        header.dst_ip
    };
    let index = if let Some(index) = udp.iter().position(|flow| {
        flow.guest_port == header.src_port
            && flow.reply_ip == header.dst_ip
            && flow.reply_port == header.dst_port
    }) {
        index
    } else {
        if udp.len() >= MAX_FLOWS {
            close_fd(udp[0].fd);
            udp.swap_remove(0);
        }
        if udp.len() >= MAX_FLOWS {
            return;
        }
        let Ok(fd) = udp_open() else {
            return;
        };
        udp.push(UdpFlow {
            fd,
            guest_mac: header.guest_mac,
            guest_ip: header.guest_ip,
            guest_port: header.src_port,
            reply_ip: header.dst_ip,
            reply_port: header.dst_port,
            real_ip,
            real_port: header.dst_port,
        });
        udp.len() - 1
    };
    let flow = &udp[index];
    let _ = send_to(flow.fd, payload, flow.real_ip, flow.real_port);
}

fn reclaim_tcp_slot(tcp: &mut Vec<TcpFlow>) {
    // Prefer finished Closing flows, then the oldest table entry.
    let index = tcp
        .iter()
        .enumerate()
        .find(|(_, flow)| flow.phase == Phase::Closing)
        .map(|(index, _)| index)
        .unwrap_or(0);
    if index < tcp.len() {
        if tcp[index].fd >= 0 {
            close_fd(tcp[index].fd);
        }
        tcp.swap_remove(index);
    }
}

fn same_tcp(flow: &TcpFlow, header: &ForwardHeader) -> bool {
    flow.guest_port == header.src_port
        && flow.dest_ip == header.dst_ip
        && flow.dest_port == header.dst_port
}

fn poll_io(tcp: &mut Vec<TcpFlow>, udp: &mut Vec<UdpFlow>, outgoing: &OutQueues) {
    // Map poll slot -> tcp index for flows that already have a host fd.
    let mut tcp_poll_index = Vec::new();
    let mut polls = Vec::with_capacity(tcp.len() + udp.len());
    for (index, flow) in tcp.iter().enumerate() {
        if flow.fd < 0 {
            continue;
        }
        let events = if flow.phase == Phase::Connecting || !flow.pending_out.is_empty() {
            POLLIN | POLLOUT
        } else {
            POLLIN
        };
        tcp_poll_index.push(index);
        polls.push(PollFd {
            fd: flow.fd,
            events,
            revents: 0,
        });
    }
    let tcp_polls = polls.len();
    for flow in udp.iter() {
        polls.push(PollFd {
            fd: flow.fd,
            events: POLLIN,
            revents: 0,
        });
    }
    if !polls.is_empty() {
        let _ = poll_fds(&mut polls);
    }
    let mut dead_tcp = Vec::new();
    for (index, flow) in tcp.iter_mut().enumerate() {
        // Retry SYN-ACK while waiting for the guest ACK (no host fd yet).
        if flow.phase == Phase::SynReceived {
            flow.synack_retries = flow.synack_retries.wrapping_add(1);
            if !flow.syn_acked || flow.synack_retries % 10 == 0 {
                let _ = send_syn_ack(outgoing, flow);
            }
            continue;
        }
        let Some(poll_slot) = tcp_poll_index.iter().position(|&i| i == index) else {
            continue;
        };
        let revents = polls[poll_slot].revents;
        if flow.phase == Phase::Connecting && (revents & (POLLERR | POLLHUP)) != 0 {
            NET_CONNECT_FAIL.fetch_add(1, Ordering::Relaxed);
            let _ = send_rst(outgoing, flow);
            dead_tcp.push(index);
            continue;
        }
        if flow.phase == Phase::Connecting && (revents & POLLOUT) != 0 {
            // Darwin/Linux: POLLOUT alone is not success; check SO_ERROR.
            let err = socket_error(flow.fd);
            if err != 0 {
                NET_CONNECT_FAIL.fetch_add(1, Ordering::Relaxed);
                eprintln!(
                    "relay-net: connect fail :{} so_error={err}",
                    flow.guest_port
                );
                let _ = send_rst(outgoing, flow);
                dead_tcp.push(index);
                continue;
            }
            flow.phase = Phase::Open;
            NET_CONNECT_OK.fetch_add(1, Ordering::Relaxed);
            eprintln!(
                "relay-net: host_open :{} pending_out={}",
                flow.guest_port,
                flow.pending_out.len()
            );
            flush_out(flow);
        }
        if (revents & POLLOUT) != 0 {
            flush_out(flow);
        }
        if flow.phase == Phase::Open || flow.phase == Phase::Closing {
            if (revents & POLLIN) != 0 && flow.phase == Phase::Open {
                while flow.inflight.len() < receive_cap(flow) {
                    let mut buf = [0u8; 1460];
                    match read_fd(flow.fd, &mut buf) {
                        Read::Bytes(data) => {
                            let first = NET_HOST_RX.fetch_add(data.len() as u64, Ordering::Relaxed)
                                == 0;
                            if first {
                                eprintln!(
                                    "relay-net: host_rx :{} first={}",
                                    flow.guest_port,
                                    data.len()
                                );
                            }
                            flow.inflight.extend_from_slice(data);
                        }
                        Read::End => {
                            flow.peer_fin = true;
                            break;
                        }
                        Read::Again => break,
                    }
                }
            }
            pump_to_guest(outgoing, flow);
            if flow.queued > 0 {
                flow.idle_polls = flow.idle_polls.saturating_add(1);
                if flow.idle_polls >= 10_000 {
                    flow.idle_polls = 0;
                    retransmit_head(outgoing, flow);
                }
            }
        }
        if flow.phase == Phase::Open
            && tcp_may_send_fin(
                flow.peer_fin,
                flow.sent_fin,
                flow.inflight.is_empty(),
                flow.queued == 0,
                true,
            )
        {
            let seq = flow.una.wrapping_add(flow.inflight.len() as u32);
            if send_tcp(outgoing, flow, seq, TCP_FIN | TCP_ACK, &[]) {
                flow.our_next = seq.wrapping_add(1);
                flow.sent_fin = true;
                flow.phase = Phase::Closing;
            }
        }
        if flow.phase == Phase::Closing && flow.sent_fin && flow.una == flow.our_next {
            dead_tcp.push(index);
        }
    }
    for index in dead_tcp.into_iter().rev() {
        if index < tcp.len() {
            if tcp[index].fd >= 0 {
                close_fd(tcp[index].fd);
            }
            tcp.swap_remove(index);
        }
    }
    let mut dead_udp = Vec::new();
    for (index, flow) in udp.iter().enumerate() {
        if tcp_polls + index >= polls.len()
            || (polls[tcp_polls + index].revents & POLLIN) == 0
        {
            continue;
        }
        let mut buf = [0u8; 1400];
        match read_fd(flow.fd, &mut buf) {
            Read::Bytes(data) => {
                if let Some(frame) = udp_ethernet(
                    flow.guest_mac,
                    flow.guest_ip,
                    flow.guest_port,
                    flow.reply_ip,
                    flow.reply_port,
                    data,
                ) {
                    let _ = outgoing.send_data(frame);
                }
            }
            Read::End => dead_udp.push(index),
            Read::Again => {}
        }
    }
    for index in dead_udp.into_iter().rev() {
        close_fd(udp[index].fd);
        udp.swap_remove(index);
    }
}

fn flush_out(flow: &mut TcpFlow) {
    if flow.pending_out.is_empty() || flow.phase != Phase::Open || flow.fd < 0 {
        return;
    }
    match write_fd(flow.fd, &flow.pending_out) {
        Some(n) if n > 0 => {
            let first = NET_HOST_TX.fetch_add(n as u64, Ordering::Relaxed) == 0;
            flow.pending_out.drain(..n);
            if first {
                eprintln!(
                    "relay-net: host_tx :{} bytes={n} left={}",
                    flow.guest_port,
                    flow.pending_out.len()
                );
            }
        }
        _ => {
            let err = errno();
            if err != EAGAIN && err != 0 {
                eprintln!(
                    "relay-net: host_write fail :{} errno={err} pending={}",
                    flow.guest_port,
                    flow.pending_out.len()
                );
            }
        }
    }
}

fn note_ack(flow: &mut TcpFlow, view: &crate::net_frame::TcpView<'_>) {
    if (view.flags & TCP_ACK) == 0 {
        return;
    }
    flow.guest_window = u32::from(view.window) << flow.window_scale;
    let delta = view.ack.wrapping_sub(flow.una);
    if delta == 0 {
        if flow.queued > 0 {
            flow.dup_acks = flow.dup_acks.saturating_add(1);
            if flow.dup_acks >= 3 {
                flow.queued = 0;
                flow.dup_acks = 0;
                flow.our_next = flow.una;
            }
        }
        return;
    }
    let fin_ack = flow.sent_fin && delta == flow.queued as u32 + 1;
    if delta > flow.queued as u32 && !fin_ack {
        return;
    }
    let n = if fin_ack {
        flow.queued
    } else {
        delta as usize
    };
    if n > 0 {
        flow.inflight.drain(..n);
        flow.queued -= n;
    }
    flow.una = view.ack;
    flow.dup_acks = 0;
    flow.idle_polls = 0;
    if !flow.sent_fin {
        flow.our_next = flow.una.wrapping_add(flow.queued as u32);
    }
}

fn receive_cap(flow: &TcpFlow) -> usize {
    (flow.guest_window.max(1460) as usize).min(128 * 1024)
}

fn pump_to_guest(outgoing: &OutQueues, flow: &mut TcpFlow) {
    if !matches!(flow.phase, Phase::Open | Phase::Closing) {
        return;
    }
    // 128-byte payloads stay in the same virtio RX buffer class as DNS and
    // SYN-ACK. A 1460-byte segment was completed into the used ring (rx del
    // rose, IRQ 67 was taken) and the guest IP stack still counted ~124 bytes.
    let chunk = usize::from(flow.mss).clamp(1, 128);
    let window = (flow.guest_window.max(chunk as u32) as usize).min(64 * 1024);
    if flow.queued == 0 && !flow.inflight.is_empty() {
        eprintln!(
            "relay-net: pump :{} window={} inflight={} chunk={}",
            flow.guest_port, window, flow.inflight.len(), chunk
        );
    }
    while flow.queued < flow.inflight.len() && flow.queued < window {
        let start = flow.queued;
        let end = (start + chunk).min(flow.inflight.len()).min(window);
        if end <= start {
            break;
        }
        let seq = flow.una.wrapping_add(start as u32);
        let data = flow.inflight[start..end].to_vec();
        if !send_tcp(outgoing, flow, seq, TCP_ACK | TCP_PSH, &data) {
            break;
        }
        flow.queued = end;
        flow.our_next = flow.una.wrapping_add(end as u32);
    }
}

fn retransmit_head(outgoing: &OutQueues, flow: &TcpFlow) {
    if flow.inflight.is_empty() {
        return;
    }
    let end = usize::from(flow.mss).clamp(1, 128).min(flow.inflight.len());
    let data = flow.inflight[..end].to_vec();
    let _ = send_tcp(outgoing, flow, flow.una, TCP_ACK | TCP_PSH, &data);
}

fn send_tcp(outgoing: &OutQueues, flow: &TcpFlow, seq: u32, flags: u8, payload: &[u8]) -> bool {
    let Some(frame) = tcp_ethernet(
        flow.guest_mac,
        flow.guest_ip,
        flow.guest_port,
        flow.dest_ip,
        flow.dest_port,
        seq,
        flow.guest_next,
        flags,
        payload,
    ) else {
        return false;
    };
    outgoing.send_data(frame)
}

fn send_syn_ack(outgoing: &OutQueues, flow: &mut TcpFlow) -> bool {
    let Some(frame) = tcp_ethernet(
        flow.guest_mac,
        flow.guest_ip,
        flow.guest_port,
        flow.dest_ip,
        flow.dest_port,
        flow.our_isn,
        flow.guest_next,
        TCP_SYN | TCP_ACK,
        &[],
    ) else {
        return false;
    };
    // Control frames share the DNS priority queue so a TCP data backlog cannot
    // bury the handshake (auth24-26: dns ok, curl connect timeout).
    if outgoing.send_prio(frame) {
        let first = !flow.syn_acked;
        flow.syn_acked = true;
        NET_SYNACK_OK.fetch_add(1, Ordering::Relaxed);
        if first || flow.synack_retries % 500 == 0 {
            eprintln!(
                "relay-net: SYN-ACK :{} <- {}.{}.{}.{}:{} retry={}",
                flow.guest_port,
                flow.dest_ip[0],
                flow.dest_ip[1],
                flow.dest_ip[2],
                flow.dest_ip[3],
                flow.dest_port,
                flow.synack_retries
            );
        }
        true
    } else {
        NET_SYNACK_DROP.fetch_add(1, Ordering::Relaxed);
        false
    }
}

fn send_rst(outgoing: &OutQueues, flow: &TcpFlow) -> bool {
    NET_RST.fetch_add(1, Ordering::Relaxed);
    let Some(frame) = tcp_ethernet(
        flow.guest_mac,
        flow.guest_ip,
        flow.guest_port,
        flow.dest_ip,
        flow.dest_port,
        flow.our_next,
        flow.guest_next,
        TCP_RST | TCP_ACK,
        &[],
    ) else {
        return false;
    };
    outgoing.send_prio(frame)
}

fn find_tcp(tcp: &[TcpFlow], header: &ForwardHeader) -> Option<usize> {
    tcp.iter().position(|flow| same_tcp(flow, header))
}

enum Read<'a> {
    Bytes(&'a [u8]),
    End,
    Again,
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

unsafe extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn connect(fd: i32, addr: *const u8, len: u32) -> i32;
    fn bind(fd: i32, addr: *const u8, len: u32) -> i32;
    fn getsockopt(
        fd: i32,
        level: i32,
        optname: i32,
        optval: *mut u8,
        optlen: *mut u32,
    ) -> i32;
    fn setsockopt(fd: i32, level: i32, optname: i32, optval: *const u8, optlen: u32) -> i32;
    fn sendto(
        fd: i32,
        buf: *const u8,
        len: usize,
        flags: i32,
        addr: *const u8,
        addrlen: u32,
    ) -> isize;
    fn read(fd: i32, buf: *mut u8, len: usize) -> isize;
    fn write(fd: i32, buf: *const u8, len: usize) -> isize;
    fn close(fd: i32) -> i32;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
    fn shutdown(fd: i32, how: i32) -> i32;
}

#[cfg(any(target_os = "linux", target_os = "android"))]
unsafe extern "C" {
    fn __errno_location() -> *mut i32;
    fn poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
}
#[cfg(not(any(target_os = "linux", target_os = "android")))]
unsafe extern "C" {
    fn __error() -> *mut i32;
    fn poll(fds: *mut PollFd, nfds: u32, timeout: i32) -> i32;
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(any(target_os = "linux", target_os = "android"))]
const EINPROGRESS: i32 = 115;
#[cfg(any(target_os = "linux", target_os = "android"))]
const EAGAIN: i32 = 11;
#[cfg(any(target_os = "linux", target_os = "android"))]
const SOL_SOCKET: i32 = 1;
#[cfg(any(target_os = "linux", target_os = "android"))]
const SO_ERROR: i32 = 4;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const O_NONBLOCK: i32 = 4;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const EINPROGRESS: i32 = 36;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const EAGAIN: i32 = 35;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const SOL_SOCKET: i32 = 0xffff;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const SO_ERROR: i32 = 0x1007;

fn socket_error(fd: i32) -> i32 {
    let mut err: i32 = 0;
    let mut len: u32 = 4;
    let rc = unsafe {
        getsockopt(
            fd,
            SOL_SOCKET,
            SO_ERROR,
            (&mut err as *mut i32).cast(),
            &mut len,
        )
    };
    if rc != 0 {
        errno()
    } else {
        err
    }
}

fn errno() -> i32 {
    unsafe {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            *__errno_location()
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            *__error()
        }
    }
}

fn sockaddr_in(ip: [u8; 4], port: u16) -> [u8; 16] {
    let mut raw = [0u8; 16];
    let port = port.to_be_bytes();
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        raw[0] = 2;
        raw[2] = port[0];
        raw[3] = port[1];
        raw[4..8].copy_from_slice(&ip);
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        raw[0] = 16;
        raw[1] = 2;
        raw[2] = port[0];
        raw[3] = port[1];
        raw[4..8].copy_from_slice(&ip);
    }
    raw
}

fn set_nonblock(fd: i32) -> bool {
    let flags = unsafe { fcntl(fd, 3, 0) };
    flags >= 0 && unsafe { fcntl(fd, 4, flags | O_NONBLOCK) } >= 0
}

fn tcp_open(ip: [u8; 4], port: u16) -> Result<i32, ()> {
    let fd = unsafe { socket(2, 1, 6) };
    if fd < 0 || !set_nonblock(fd) {
        if fd >= 0 {
            close_fd(fd);
        }
        return Err(());
    }
    let addr = sockaddr_in(ip, port);
    let rc = unsafe { connect(fd, addr.as_ptr(), 16) };
    let connect_err = if rc == 0 { 0 } else { errno() };
    let one: i32 = 1;
    unsafe {
        setsockopt(fd, 6, 1, (&one as *const i32).cast(), 4);
    }
    if rc == 0 || connect_err == EINPROGRESS {
        Ok(fd)
    } else {
        close_fd(fd);
        Err(())
    }
}

fn udp_open() -> Result<i32, ()> {
    let fd = unsafe { socket(2, 2, 17) };
    if fd < 0 || !set_nonblock(fd) {
        if fd >= 0 {
            close_fd(fd);
        }
        return Err(());
    }
    let addr = sockaddr_in([0, 0, 0, 0], 0);
    if unsafe { bind(fd, addr.as_ptr(), 16) } != 0 {
        close_fd(fd);
        return Err(());
    }
    Ok(fd)
}

fn send_to(fd: i32, payload: &[u8], ip: [u8; 4], port: u16) -> bool {
    let addr = sockaddr_in(ip, port);
    let sent = unsafe { sendto(fd, payload.as_ptr(), payload.len(), 0, addr.as_ptr(), 16) };
    sent == payload.len() as isize
}

fn read_fd<'a>(fd: i32, buf: &'a mut [u8]) -> Read<'a> {
    let n = unsafe { read(fd, buf.as_mut_ptr(), buf.len()) };
    if n > 0 {
        Read::Bytes(&buf[..n as usize])
    } else if n == 0 {
        Read::End
    } else if errno() == EAGAIN {
        Read::Again
    } else {
        Read::End
    }
}

fn write_fd(fd: i32, buf: &[u8]) -> Option<usize> {
    let n = unsafe { write(fd, buf.as_ptr(), buf.len()) };
    if n > 0 {
        Some(n as usize)
    } else {
        None
    }
}

fn shutdown_write(fd: i32) {
    unsafe {
        shutdown(fd, 1);
    }
}

fn close_fd(fd: i32) {
    unsafe {
        close(fd);
    }
}

fn poll_fds(fds: &mut [PollFd]) -> i32 {
    unsafe {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            poll(fds.as_mut_ptr(), fds.len() as u64, 0)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            poll(fds.as_mut_ptr(), fds.len() as u32, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net_frame::{guest_tcp, GUEST_IP, GUEST_MAC};
    use std::io::Write as _;
    use std::net::TcpListener;

    #[test]
    fn tcp_loopback_returns_the_server_payload() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = Worker::spawn();
        let syn = guest_tcp(
            GUEST_MAC,
            GUEST_IP,
            40000,
            [127, 0, 0, 1],
            port,
            1000,
            0,
            TCP_SYN,
            &[],
        )
        .unwrap();
        worker.submit(syn);
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut saw_syn_ack = false;
        let mut queue = VecDeque::new();
        // Lazy host connect: ACK the SYN-ACK before the listener can accept.
        while !saw_syn_ack && std::time::Instant::now() < deadline {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if frame.len() < 14 + 40 {
                    continue;
                }
                let flags = frame[14 + 20 + 13];
                if (flags & TCP_SYN) != 0 && (flags & TCP_ACK) != 0 {
                    saw_syn_ack = true;
                    let ack_num =
                        u32::from_be_bytes(frame[14 + 20 + 4..14 + 20 + 8].try_into().unwrap())
                            .wrapping_add(1);
                    worker.submit(
                        guest_tcp(
                            GUEST_MAC,
                            GUEST_IP,
                            40000,
                            [127, 0, 0, 1],
                            port,
                            1001,
                            ack_num,
                            TCP_ACK,
                            &[],
                        )
                        .unwrap(),
                    );
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(saw_syn_ack, "guest never saw SYN-ACK");
        let mut accepted = None;
        while accepted.is_none() && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => accepted = Some(stream),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        }
        let mut stream = accepted.expect("host listener did not accept");
        stream.set_nonblocking(false).unwrap();
        stream.write_all(b"pong").unwrap();
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut saw_pong = false;
        while std::time::Instant::now() < deadline && !saw_pong {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if frame.len() < 14 + 40 {
                    continue;
                }
                let header_len = ((frame[14 + 20 + 12] >> 4) as usize) * 4;
                let ip_total = u16::from_be_bytes(frame[16..18].try_into().unwrap()) as usize;
                let data_start = 14 + 20 + header_len;
                let data_end = 14 + ip_total;
                if data_end > data_start && data_end <= frame.len() {
                    if frame[data_start..data_end].windows(4).any(|window| window == *b"pong") {
                        saw_pong = true;
                    }
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        drop(stream);
        assert!(saw_pong, "guest never saw server payload");
    }

    #[test]
    fn tcp_loopback_delivers_64kib_without_dropping_the_fin() {
        let payload = vec![0x5a; 64 * 1024];
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = Worker::spawn();
        let syn = guest_tcp(
            GUEST_MAC,
            GUEST_IP,
            40010,
            [127, 0, 0, 1],
            port,
            2000,
            0,
            TCP_SYN,
            &[],
        )
        .unwrap();
        worker.submit(syn);
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut seq = 2001u32;
        let mut ack = 0u32;
        let mut queue = VecDeque::new();
        let mut saw_syn_ack = false;
        while !saw_syn_ack && std::time::Instant::now() < deadline {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if frame.len() < 14 + 40 {
                    continue;
                }
                let flags = frame[14 + 20 + 13];
                let seg_seq =
                    u32::from_be_bytes(frame[14 + 20 + 4..14 + 20 + 8].try_into().unwrap());
                if (flags & TCP_SYN) != 0 && (flags & TCP_ACK) != 0 {
                    ack = seg_seq.wrapping_add(1);
                    saw_syn_ack = true;
                    worker.submit(
                        guest_tcp(
                            GUEST_MAC,
                            GUEST_IP,
                            40010,
                            [127, 0, 0, 1],
                            port,
                            seq,
                            ack,
                            TCP_ACK,
                            &[],
                        )
                        .unwrap(),
                    );
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(saw_syn_ack, "guest never saw SYN-ACK");
        let mut accepted = None;
        while accepted.is_none() && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => accepted = Some(stream),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        }
        let mut stream = accepted.expect("host listener did not accept");
        stream.set_nonblocking(false).unwrap();
        stream.write_all(&payload).unwrap();
        drop(stream);
        let mut received = Vec::new();
        while std::time::Instant::now() < deadline && received.len() < payload.len() {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if frame.len() < 14 + 40 {
                    continue;
                }
                let seg_seq =
                    u32::from_be_bytes(frame[14 + 20 + 4..14 + 20 + 8].try_into().unwrap());
                let header_len = ((frame[14 + 20 + 12] >> 4) as usize) * 4;
                let ip_total = u16::from_be_bytes(frame[16..18].try_into().unwrap()) as usize;
                let data_start = 14 + 20 + header_len;
                let data_end = 14 + ip_total;
                if data_end > data_start && data_end <= frame.len() {
                    received.extend_from_slice(&frame[data_start..data_end]);
                    ack = seg_seq.wrapping_add((data_end - data_start) as u32);
                    worker.submit(
                        guest_tcp(
                            GUEST_MAC,
                            GUEST_IP,
                            40010,
                            [127, 0, 0, 1],
                            port,
                            seq,
                            ack,
                            TCP_ACK,
                            &[],
                        )
                        .unwrap(),
                    );
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(received, payload);
    }

    fn dns_query(name: &str) -> Vec<u8> {
        let mut query = vec![0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
        for label in name.split('.') {
            query.push(label.len() as u8);
            query.extend_from_slice(label.as_bytes());
        }
        query.push(0);
        query.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);
        query
    }

    fn skip_name(payload: &[u8], mut index: usize) -> Option<usize> {
        let mut hops = 0;
        while index < payload.len() && hops < 16 {
            let len = payload[index] as usize;
            if len == 0 {
                return Some(index + 1);
            }
            if payload[index] & 0xc0 == 0xc0 {
                return Some(index + 2);
            }
            if index + 1 + len > payload.len() {
                return None;
            }
            index += 1 + len;
            hops += 1;
        }
        None
    }

    fn first_a_record(payload: &[u8]) -> Option<[u8; 4]> {
        if payload.len() < 12 || payload[2] & 0x80 == 0 {
            return None;
        }
        let mut index = skip_name(payload, 12)?;
        index += 4;
        while index + 10 <= payload.len() {
            index = skip_name(payload, index)?;
            if index + 10 > payload.len() {
                return None;
            }
            let kind = u16::from_be_bytes([payload[index], payload[index + 1]]);
            let rdlen = u16::from_be_bytes([payload[index + 8], payload[index + 9]]) as usize;
            index += 10;
            if index + rdlen > payload.len() {
                return None;
            }
            if kind == 1 && rdlen == 4 {
                return Some(payload[index..index + 4].try_into().unwrap());
            }
            index += rdlen;
        }
        None
    }

    #[test]
    fn nat_resolves_cache_nixos_org_and_opens_tcp_443() {
        use crate::net_frame::{guest_udp, DNS_IP};
        let query = guest_udp(GUEST_MAC, GUEST_IP, 53000, DNS_IP, 53, &dns_query("cache.nixos.org"))
            .unwrap();
        let worker = Worker::spawn();
        worker.submit(query);
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut address = None;
        let mut queue = VecDeque::new();
        while address.is_none() && std::time::Instant::now() < deadline {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if let Some(payload) = crate::net_frame::udp_payload(&frame) {
                    address = first_a_record(payload);
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        let address = address.expect("NAT DNS returned no A record for cache.nixos.org");
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let syn = guest_tcp(GUEST_MAC, GUEST_IP, 53001, address, 443, 2000, 0, TCP_SYN, &[]).unwrap();
        worker.submit(syn);
        let mut saw_syn_ack = false;
        while !saw_syn_ack && std::time::Instant::now() < deadline {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if frame.len() >= 14 + 40 {
                    let flags = frame[14 + 20 + 13];
                    if flags & TCP_SYN != 0 && flags & TCP_ACK != 0 {
                        saw_syn_ack = true;
                    }
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_syn_ack, "NAT did not open TCP 443 to {address:?}");
    }

    #[test]
    fn nat_tls_client_hello_returns_server_bytes() {
        use crate::net_frame::{guest_udp, DNS_IP};
        let query = guest_udp(GUEST_MAC, GUEST_IP, 53100, DNS_IP, 53, &dns_query("cache.nixos.org"))
            .unwrap();
        let worker = Worker::spawn();
        worker.submit(query);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut address = None;
        let mut queue = VecDeque::new();
        while address.is_none() && std::time::Instant::now() < deadline {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if let Some(payload) = crate::net_frame::udp_payload(&frame) {
                    address = first_a_record(payload);
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        let address = address.expect("NAT DNS returned no A record");
        let syn = guest_tcp(GUEST_MAC, GUEST_IP, 53101, address, 443, 3000, 0, TCP_SYN, &[]).unwrap();
        worker.submit(syn);
        let mut ack_num = 0u32;
        let mut saw_syn_ack = false;
        while !saw_syn_ack && std::time::Instant::now() < deadline {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if frame.len() < 14 + 40 {
                    continue;
                }
                let flags = frame[14 + 20 + 13];
                if flags & TCP_SYN != 0 && flags & TCP_ACK != 0 {
                    ack_num = u32::from_be_bytes(frame[14 + 20 + 4..14 + 20 + 8].try_into().unwrap())
                        .wrapping_add(1);
                    saw_syn_ack = true;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(saw_syn_ack, "no SYN-ACK from cache.nixos.org");
        let hello = tls_client_hello(b"cache.nixos.org");
        worker.submit(
            guest_tcp(
                GUEST_MAC,
                GUEST_IP,
                53101,
                address,
                443,
                3001,
                ack_num,
                TCP_ACK | TCP_PSH,
                &hello,
            )
            .unwrap(),
        );
        let mut got = 0usize;
        while got == 0 && std::time::Instant::now() < deadline {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if frame.len() < 14 + 40 {
                    continue;
                }
                let header_len = ((frame[14 + 20 + 12] >> 4) as usize) * 4;
                let ip_total = u16::from_be_bytes(frame[16..18].try_into().unwrap()) as usize;
                let data_start = 14 + 20 + header_len;
                let data_end = 14 + ip_total;
                if data_end > data_start && data_end <= frame.len() {
                    got += data_end - data_start;
                    let ack = u32::from_be_bytes(frame[14 + 20 + 4..14 + 20 + 8].try_into().unwrap())
                        .wrapping_add((data_end - data_start) as u32);
                    worker.submit(
                        guest_tcp(
                            GUEST_MAC,
                            GUEST_IP,
                            53101,
                            address,
                            443,
                            3001u32.wrapping_add(hello.len() as u32),
                            ack,
                            TCP_ACK,
                            &[],
                        )
                        .unwrap(),
                    );
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(got > 0, "cache.nixos.org returned no TLS bytes through NAT");
    }

    fn tls_client_hello(server: &[u8]) -> Vec<u8> {
        let mut sni_body = Vec::new();
        sni_body.extend_from_slice(&((server.len() + 3) as u16).to_be_bytes());
        sni_body.push(0);
        sni_body.extend_from_slice(&(server.len() as u16).to_be_bytes());
        sni_body.extend_from_slice(server);
        let mut ext = Vec::new();
        ext.extend_from_slice(&0u16.to_be_bytes());
        ext.extend_from_slice(&(sni_body.len() as u16).to_be_bytes());
        ext.extend_from_slice(&sni_body);
        let mut hello = Vec::new();
        hello.extend_from_slice(&[0x03, 0x03]);
        hello.extend_from_slice(&[0x11; 32]);
        hello.push(0);
        hello.extend_from_slice(&[0x00, 0x02, 0x00, 0x2f]);
        hello.extend_from_slice(&[0x01, 0x00]);
        hello.extend_from_slice(&(ext.len() as u16).to_be_bytes());
        hello.extend_from_slice(&ext);
        let mut rec = vec![0x16, 0x03, 0x01, 0, 0, 0x01];
        let hs_len = hello.len() as u32;
        rec.push((hs_len >> 16) as u8);
        rec.push((hs_len >> 8) as u8);
        rec.push(hs_len as u8);
        rec.extend_from_slice(&hello);
        let body = (rec.len() - 5) as u16;
        rec[3] = (body >> 8) as u8;
        rec[4] = body as u8;
        rec
    }

    #[test]
    fn nat_resolves_channels_nixos_org() {
        use crate::net_frame::{guest_udp, DNS_IP};
        let query =
            guest_udp(GUEST_MAC, GUEST_IP, 53010, DNS_IP, 53, &dns_query("channels.nixos.org"))
                .unwrap();
        let worker = Worker::spawn();
        worker.submit(query);
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut address = None;
        let mut queue = VecDeque::new();
        while address.is_none() && std::time::Instant::now() < deadline {
            worker.drain(&mut queue);
            while let Some(frame) = queue.pop_front() {
                if let Some(payload) = crate::net_frame::udp_payload(&frame) {
                    address = first_a_record(payload);
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            address.is_some(),
            "NAT DNS returned no A record for channels.nixos.org"
        );
    }
}

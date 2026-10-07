//! Virtio-net (device id 1) over MMIO. Queue 0 is guest receive, queue 1 is
//! guest transmit. Replies that do not need a host socket (ARP, DHCP, ICMP
//! to the gateway) are generated here. TCP and UDP go to the socket worker.

use crate::guest::GuestMemory;
use crate::net_frame::{self, Ingress};
use crate::virtio_block::parse_chain;
use crate::virtio_mmio::Transport;
use relay_core::RelayError;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[cfg(not(kani))]
use crate::net_host::Worker;

const MAX_CHAIN: usize = 65536;

static NET_RX_DELIVERED: AtomicU64 = AtomicU64::new(0);
static NET_RX_NO_BUF: AtomicU64 = AtomicU64::new(0);
static NET_RX_DROP_SMALL: AtomicU64 = AtomicU64::new(0);
static NET_RX_WIDE_LOGGED: AtomicBool = AtomicBool::new(false);
static NET_RX_DELIVER_LOGGED: AtomicBool = AtomicBool::new(false);

pub fn rx_counters() -> (u64, u64, u64) {
    (
        NET_RX_DELIVERED.load(Ordering::Relaxed),
        NET_RX_NO_BUF.load(Ordering::Relaxed),
        NET_RX_DROP_SMALL.load(Ordering::Relaxed),
    )
}

pub(crate) struct Net {
    rx: VecDeque<Vec<u8>>,
    #[cfg(not(kani))]
    worker: Option<Worker>,
}

impl Default for Net {
    fn default() -> Self {
        Self {
            rx: VecDeque::new(),
            #[cfg(not(kani))]
            worker: None,
        }
    }
}

impl Net {
    pub(crate) fn rx_pending(&self) -> bool {
        !self.rx.is_empty()
    }

    pub(crate) fn service(
        &mut self,
        transport: &mut Transport,
        memory: &mut GuestMemory,
    ) -> Result<usize, RelayError> {
        #[cfg(not(kani))]
        if let Some(worker) = &self.worker {
            worker.drain(&mut self.rx);
        }
        let mut completed = 0;
        if transport.queue_operational(1) {
            completed += self.transmit(transport, memory)?;
        }
        if transport.queue_operational(0) {
            completed += self.receive(transport, memory)?;
        }
        Ok(completed)
    }

    fn transmit(
        &mut self,
        transport: &mut Transport,
        memory: &mut GuestMemory,
    ) -> Result<usize, RelayError> {
        let mut completed = 0;
        loop {
            let Some(head) = transport.pop_available(1, memory)? else {
                transport.clear_notification(1);
                break;
            };
            let chain = parse_chain(
                memory,
                transport.descriptor_table(1)?,
                head,
                transport.queue_size(1)?,
            )?;
            let mut raw = Vec::new();
            for descriptor in &chain {
                if descriptor.writable {
                    return Err(RelayError::Failed(
                        "virtio net transmit descriptor is writable".into(),
                    ));
                }
                if raw.len() >= MAX_CHAIN {
                    break;
                }
                let room = MAX_CHAIN - raw.len();
                let take = (descriptor.length as usize).min(room);
                let start = raw.len();
                raw.resize(start + take, 0);
                memory.read(descriptor.address, &mut raw[start..])?;
            }
            if let Some(frame) = net_frame::ethernet_from_virtio(&raw) {
                self.ingest(frame);
            }
            transport.complete(1, memory, head, 0)?;
            completed += 1;
        }
        Ok(completed)
    }

    fn receive(
        &mut self,
        transport: &mut Transport,
        memory: &mut GuestMemory,
    ) -> Result<usize, RelayError> {
        let mut completed = 0;
        while !self.rx.is_empty() {
            let header = if transport.negotiated_features() & net_frame::VIRTIO_F_MRG_RXBUF != 0
            {
                net_frame::VIRTIO_RX_HEADER
            } else {
                10
            };
            let needed = header
                + self
                    .rx
                    .front()
                    .map(|frame| frame.len())
                    .unwrap_or(0);
            let Some(head) = transport.pop_available(0, memory)? else {
                NET_RX_NO_BUF.fetch_add(1, Ordering::Relaxed);
                break;
            };
            let chain = parse_chain(
                memory,
                transport.descriptor_table(0)?,
                head,
                transport.queue_size(0)?,
            )?;
            // Linux mergeable RX copies `used.len` bytes from the first
            // descriptor only when num_buffers is 1. Spilling the rest of a
            // 1514-byte frame into a NEXT descriptor made the IP header
            // claim bytes the driver never reads. SYN-ACK fits in that first
            // descriptor, so the handshake succeeded and the TLS flight did
            // not. Return a too-small buffer and keep the frame for a later
            // one instead of dropping it.
            let first = chain.first().map(|descriptor| descriptor.length as usize).unwrap_or(0);
            let first_writable = chain.first().is_some_and(|descriptor| descriptor.writable);
            if !first_writable || first < needed {
                if !NET_RX_WIDE_LOGGED.swap(true, Ordering::Relaxed) {
                    eprintln!(
                        "relay-net: rx buffer short need={needed} first={first} chain={}",
                        chain.len()
                    );
                }
                NET_RX_DROP_SMALL.fetch_add(1, Ordering::Relaxed);
                transport.complete(0, memory, head, 0)?;
                completed += 1;
                continue;
            }
            let frame = self.rx.pop_front().expect("front existed");
            let packet = net_frame::rx_virtio_packet(&frame, transport.negotiated_features());
            if frame.len() > 100 && !NET_RX_DELIVER_LOGGED.swap(true, Ordering::Relaxed) {
                let n = frame.len().min(40);
                eprintln!(
                    "relay-net: rx deliver frame={} pkt={} first={} head={:02x?}",
                    frame.len(),
                    packet.len(),
                    first,
                    &frame[..n]
                );
            }
            memory.write(chain[0].address, &packet)?;
            transport.complete(0, memory, head, packet.len() as u32)?;
            NET_RX_DELIVERED.fetch_add(1, Ordering::Relaxed);
            completed += 1;
        }
        Ok(completed)
    }

    fn ingest(&mut self, frame: &[u8]) {
        let Some(frame) = frame.get(..frame.len().min(net_frame::MAX_FRAME)) else {
            return;
        };
        match net_frame::classify(frame) {
            Some(Ingress::Local(reply)) => {
                // Stay under the NAT drain CAP so ARP/DHCP cannot bury SYN-ACK.
                if self.rx.len() < 64 {
                    self.rx.push_back(reply);
                }
            }
            Some(Ingress::Forward) => {
                #[cfg(not(kani))]
                {
                    if self.worker.is_none() {
                        self.worker = Some(Worker::spawn());
                    }
                    if let Some(worker) = &self.worker {
                        worker.submit(frame.to_vec());
                    }
                }
            }
            None => {}
        }
    }
}

//! Fixed MMIO windows. RAM remains solely in GuestMemory.
use relay_core::RelayError;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const PL011_BASE: u64 = 0x0900_0000;
const PL011_SIZE: u64 = 0x1000;
pub(crate) const VIRTIO_BLOCK_BASE: u64 = 0x0a00_0000;
pub(crate) const VIRTIO_CONSOLE_BASE: u64 = 0x0a00_1000;
pub(crate) const VIRTIO_VSOCK_BASE: u64 = 0x0a00_2000;
pub(crate) const VIRTIO_NET_BASE: u64 = 0x0a00_3000;
const VIRTIO_MMIO_SIZE: u64 = 0x1000;
const NET_IRQ: u32 = 67;
static NET_IRQ_IARS: AtomicU64 = AtomicU64::new(0);

pub fn net_irq_iars() -> u64 {
    NET_IRQ_IARS.load(Ordering::Relaxed)
}

/// GICv2 returns the highest-priority pending interrupt. Relay programs
/// every priority register as zero, so the lowest ID would always win.
/// The architected timer is PPI 27. Virtio-net is SPI 35 (GIC 67). A
/// lowest-ID acknowledge therefore returns the timer whenever both are
/// pending, and the guest never enters the virtio-net handler after it
/// leaves the transmit poll. Shared-peripheral interrupts are acknowledged
/// first. The timer is still pending and is acknowledged on the next IAR.
fn acknowledge_pending_irq(pending: &std::collections::BTreeSet<u32>) -> u32 {
    pending
        .iter()
        .copied()
        .find(|irq| *irq >= 32)
        .or_else(|| pending.iter().copied().next())
        .unwrap_or(1023)
}
const GICD_BASE: u64 = 0x0800_0000;
const GICD_SIZE: u64 = 0x1_0000;
const GICC_BASE: u64 = 0x080a_0000;
const GICC_SIZE: u64 = 0x20_0000;
const UART_DR: u64 = 0;
const UART_FR: u64 = 0x18;
// GICv2 GICD_SGIR (Arm IHI 0048B, 4.3.15), one CPU interface, no
// Security Extensions. NSATT is reserved; SGI source CPU is always zero.
fn sgi_for_cpu0(value: u32) -> Option<u32> {
    let targeted = match (value >> 24) & 3 {
        0 => value & (1 << 16) != 0,
        2 => true,
        _ => false, // Other CPUs only, or reserved filter.
    };
    targeted.then_some(value & 15)
}

#[cfg(kani)]
#[kani::proof]
fn software_interrupt_targets_only_the_single_cpu() {
    let value: u32 = kani::any();
    let filter = (value >> 24) & 3;
    let result = sgi_for_cpu0(value);
    assert_eq!(
        result.is_some(),
        filter == 2 || (filter == 0 && value & 0x10000 != 0)
    );
    if let Some(irq) = result {
        assert!(irq < 16);
        assert_eq!(irq, value & 15);
    }
}

pub(crate) struct Bus {
    console: Vec<u8>,
    block_transport: Option<crate::virtio_mmio::Transport>,
    block: Option<crate::virtio_block::BlockDevice>,
    console_transport: crate::virtio_mmio::Transport,
    virtio_console: crate::virtio_console::Console,
    vsock_transport: crate::virtio_mmio::Transport,
    vsock: crate::vsock::Vsock,
    net_transport: crate::virtio_mmio::Transport,
    net: crate::virtio_net::Net,
    pending_irqs: std::collections::BTreeSet<u32>,
    mmio_reads: Cell<u64>,
    last_mmio_read: Cell<u64>,
    mmio_writes: u64,
    last_mmio_write: u64,
    pl011_writes: u64,
    gicc_iar_reads: Cell<u64>,
    gicc_spurious_reads: Cell<u64>,
    gicc_eoir_writes: u64,
    last_irq_ack: Cell<u64>,
    last_irq_eoi: u64,
}
impl Default for Bus {
    fn default() -> Self {
        Self {
            console: Vec::new(),
            block_transport: None,
            block: None,
            console_transport: crate::virtio_mmio::Transport::new(3, 1 << 32, 256, 2),
            virtio_console: crate::virtio_console::Console::new(1024 * 1024),
            vsock_transport: crate::virtio_mmio::Transport::new(19, 1 << 32, 256, 3),
            vsock: crate::vsock::Vsock::default(),
            net_transport: crate::virtio_mmio::Transport::new(
                1,
                // GUEST_CSUM, MAC, MRG_RXBUF, STATUS, VERSION_1.
                // GUEST_CSUM makes VIRTIO_NET_HDR_F_DATA_VALID spec-legal.
                // MRG_RXBUF makes the header 12 bytes with num_buffers. A
                // 10-byte header shifts the Ethernet frame and the guest
                // drops ARP and DNS.
                (1 << 1) | (1 << 5) | (1 << 15) | (1 << 16) | (1 << 32),
                256,
                2,
            ),
            net: crate::virtio_net::Net::default(),
            pending_irqs: std::collections::BTreeSet::new(),
            mmio_reads: Cell::new(0),
            last_mmio_read: Cell::new(0),
            mmio_writes: 0,
            last_mmio_write: 0,
            pl011_writes: 0,
            gicc_iar_reads: Cell::new(0),
            gicc_spurious_reads: Cell::new(0),
            gicc_eoir_writes: 0,
            last_irq_ack: Cell::new(1023),
            last_irq_eoi: 1023,
        }
    }
}
impl Bus {
    pub(crate) fn with_block(block: crate::virtio_block::BlockDevice) -> Self {
        Self {
            block_transport: Some(crate::virtio_mmio::Transport::new(
                2,
                (1 << 32) | (1 << 9),
                256,
                1,
            )),
            block: Some(block),
            ..Self::default()
        }
    }
    pub(crate) fn handles(&self, address: u64) -> bool {
        (PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address)
            || (GICD_BASE..GICD_BASE + GICD_SIZE).contains(&address)
            || (GICC_BASE..GICC_BASE + GICC_SIZE).contains(&address)
            || (self.block_transport.is_some()
                && (VIRTIO_BLOCK_BASE..VIRTIO_BLOCK_BASE + VIRTIO_MMIO_SIZE).contains(&address))
            || (VIRTIO_VSOCK_BASE..VIRTIO_VSOCK_BASE + VIRTIO_MMIO_SIZE).contains(&address)
            || (VIRTIO_NET_BASE..VIRTIO_NET_BASE + VIRTIO_MMIO_SIZE).contains(&address)
            || (VIRTIO_CONSOLE_BASE..VIRTIO_CONSOLE_BASE + VIRTIO_MMIO_SIZE).contains(&address)
    }
    pub(crate) fn read32(&self, address: u64) -> Result<u32, RelayError> {
        if address & 3 != 0 {
            return Err(RelayError::Failed(format!(
                "StaticCpu unmapped MMIO read {address:#x}"
            )));
        }
        self.mmio_reads.set(self.mmio_reads.get() + 1);
        self.last_mmio_read.set(address);
        if (PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address) {
            return match address - PL011_BASE {
                UART_FR | UART_DR => Ok(0),
                _ => Ok(0),
            };
        }
        if (GICD_BASE..GICD_BASE + GICD_SIZE).contains(&address) {
            return Ok(match address - GICD_BASE {
                0x004 => 2,                   // three 32-interrupt banks, enough for SPI 66
                0x100..=0x17c => 0xffff_ffff, // all SPIs enabled for the sole vCPU
                0x800..=0xbfc => 0x0101_0101, // every SPI targets CPU interface zero
                _ => 0,
            });
        }
        if (GICC_BASE..GICC_BASE + GICC_SIZE).contains(&address) {
            return Ok(match address - GICC_BASE {
                0x00c => {
                    let irq = acknowledge_pending_irq(&self.pending_irqs);
                    self.gicc_iar_reads.set(self.gicc_iar_reads.get() + 1);
                    if irq == 1023 {
                        self.gicc_spurious_reads
                            .set(self.gicc_spurious_reads.get() + 1);
                    } else if irq == NET_IRQ {
                        NET_IRQ_IARS.fetch_add(1, Ordering::Relaxed);
                    }
                    self.last_irq_ack.set(u64::from(irq));
                    irq
                }
                _ => 0,
            });
        }
        if (VIRTIO_BLOCK_BASE..VIRTIO_BLOCK_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            let offset = address - VIRTIO_BLOCK_BASE;
            if offset == 0x0fc {
                return Ok(0);
            }
            if matches!(offset, 0x100 | 0x104) {
                let sectors = self
                    .block
                    .as_ref()
                    .ok_or_else(|| RelayError::Failed("virtio block is not attached".into()))?
                    .sectors();
                return Ok(if offset == 0x100 {
                    sectors as u32
                } else {
                    (sectors >> 32) as u32
                });
            }
            return self
                .block_transport
                .as_ref()
                .ok_or_else(|| RelayError::Failed("virtio block is not attached".into()))?
                .read(offset);
        }
        if (VIRTIO_CONSOLE_BASE..VIRTIO_CONSOLE_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            return self.console_transport.read(address - VIRTIO_CONSOLE_BASE);
        }
        if (VIRTIO_VSOCK_BASE..VIRTIO_VSOCK_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            return match address - VIRTIO_VSOCK_BASE {
                0x100 => Ok(crate::vsock::GUEST_CID as u32),
                0x104 | 0x0fc => Ok(0),
                offset => self.vsock_transport.read(offset),
            };
        }
        if (VIRTIO_NET_BASE..VIRTIO_NET_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            let offset = address - VIRTIO_NET_BASE;
            if offset == 0x0fc {
                return Ok(0);
            }
            if (0x100..0x180).contains(&offset) {
                let byte = |index| u32::from(crate::net_frame::config_byte(offset + index));
                return Ok(byte(0) | (byte(1) << 8) | (byte(2) << 16) | (byte(3) << 24));
            }
            return self.net_transport.read(offset);
        }
        Err(RelayError::Failed(format!(
            "StaticCpu unmapped MMIO read {address:#x}"
        )))
    }
    pub(crate) fn write32(&mut self, address: u64, value: u32) -> Result<(), RelayError> {
        if address & 3 != 0 {
            return Err(RelayError::Failed(format!(
                "StaticCpu unmapped MMIO write {address:#x}"
            )));
        }
        self.mmio_writes += 1;
        self.last_mmio_write = address;
        if (PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address) {
            if address - PL011_BASE == UART_DR {
                self.console.push(value as u8);
                self.pl011_writes += 1;
            }
            return Ok(());
        }
        if (GICC_BASE..GICC_BASE + GICC_SIZE).contains(&address) {
            if address - GICC_BASE == 0x010 {
                self.gicc_eoir_writes += 1;
                self.last_irq_eoi = u64::from(value);
                self.pending_irqs.remove(&value);
            }
            return Ok(());
        }
        if (GICD_BASE..GICD_BASE + GICD_SIZE).contains(&address) {
            if address - GICD_BASE == 0xf00 {
                if let Some(irq) = sgi_for_cpu0(value) {
                    self.raise_irq(irq);
                }
            }
            return Ok(());
        }
        if (VIRTIO_BLOCK_BASE..VIRTIO_BLOCK_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            return self
                .block_transport
                .as_mut()
                .ok_or_else(|| RelayError::Failed("virtio block is not attached".into()))?
                .write(address - VIRTIO_BLOCK_BASE, value);
        }
        if (VIRTIO_CONSOLE_BASE..VIRTIO_CONSOLE_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            return self
                .console_transport
                .write(address - VIRTIO_CONSOLE_BASE, value);
        }
        if (VIRTIO_VSOCK_BASE..VIRTIO_VSOCK_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            let offset = address - VIRTIO_VSOCK_BASE;
            self.vsock_transport.write(offset, value)?;
            if offset == 0x070 && value == 0 {
                self.vsock.reset();
                self.pending_irqs.remove(&68);
            }
            return Ok(());
        }
        if (VIRTIO_NET_BASE..VIRTIO_NET_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            return self
                .net_transport
                .write(address - VIRTIO_NET_BASE, value);
        }
        Err(RelayError::Failed(format!(
            "StaticCpu unmapped MMIO write {address:#x}"
        )))
    }
    pub(crate) fn read8(&self, address: u64) -> Option<u8> {
        if (PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address) {
            self.mmio_reads.set(self.mmio_reads.get() + 1);
            self.last_mmio_read.set(address);
            return Some(0);
        }
        if (GICD_BASE..GICD_BASE + GICD_SIZE).contains(&address) {
            let word = self.read32(address & !3).ok()?;
            return Some((word >> ((address & 3) * 8)) as u8);
        }
        if (VIRTIO_NET_BASE..VIRTIO_NET_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            return Some(self.net_byte(address));
        }
        None
    }
    pub(crate) fn read16(&self, address: u64) -> Option<u16> {
        if (GICD_BASE..GICD_BASE + GICD_SIZE).contains(&address) {
            let word = self.read32(address & !3).ok()?;
            return Some((word >> ((address & 2) * 8)) as u16);
        }
        if (VIRTIO_NET_BASE..VIRTIO_NET_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            // virtio-net status is a u16 at config offset 6. Linux virtio-mmio
            // v2 reads that with a halfword, not a byte.
            let low = u16::from(self.net_byte(address));
            let high = u16::from(self.net_byte(address.saturating_add(1)));
            return Some(low | (high << 8));
        }
        None
    }
    fn net_byte(&self, address: u64) -> u8 {
        let offset = address - VIRTIO_NET_BASE;
        self.mmio_reads.set(self.mmio_reads.get() + 1);
        self.last_mmio_read.set(address);
        if offset >= 0x100 {
            return crate::net_frame::config_byte(offset - 0x100);
        }
        self.net_transport
            .read(offset & !3)
            .map(|word| (word >> ((offset & 3) * 8)) as u8)
            .unwrap_or(0)
    }
    pub(crate) fn write8(&mut self, address: u64, value: u8) -> bool {
        if (PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address) {
            if address - PL011_BASE == UART_DR {
                self.console.push(value);
                self.pl011_writes += 1;
            }
            self.mmio_writes += 1;
            self.last_mmio_write = address;
            return true;
        }
        if (GICD_BASE..GICD_BASE + GICD_SIZE).contains(&address) {
            self.mmio_writes += 1;
            self.last_mmio_write = address;
            return true;
        }
        if (VIRTIO_NET_BASE..VIRTIO_NET_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            self.mmio_writes += 1;
            self.last_mmio_write = address;
            return true;
        }
        false
    }
    pub(crate) fn write16(&mut self, address: u64, _value: u16) -> bool {
        if (GICD_BASE..GICD_BASE + GICD_SIZE).contains(&address) {
            self.mmio_writes += 1;
            self.last_mmio_write = address;
            return true;
        }
        if (VIRTIO_NET_BASE..VIRTIO_NET_BASE + VIRTIO_MMIO_SIZE).contains(&address) {
            self.mmio_writes += 1;
            self.last_mmio_write = address;
            return true;
        }
        false
    }
    pub(crate) fn service(
        &mut self,
        memory: &mut crate::guest::GuestMemory,
    ) -> Result<usize, RelayError> {
        let block_completed = match (&mut self.block, &mut self.block_transport) {
            (Some(block), Some(transport)) => block.process_notified_queue(transport, memory)?,
            _ => 0,
        };
        let console_completed = self
            .virtio_console
            .process_notified_queues(&mut self.console_transport, memory)?;
        if block_completed != 0 {
            // DT interrupt specifiers use SPI numbers. GIC IDs add 32.
            self.pending_irqs.insert(64);
        }
        if console_completed != 0 {
            self.pending_irqs.insert(66);
        }
        self.console
            .extend_from_slice(&self.virtio_console.take_output());
        let vsock_completed =
            crate::virtio_vsock::service(&mut self.vsock, &mut self.vsock_transport, memory)?;
        if vsock_completed != 0 || self.vsock_transport.read(0x060)? != 0 {
            self.pending_irqs.insert(68);
        } else {
            self.pending_irqs.remove(&68);
        }
        let net_completed = self.net.service(&mut self.net_transport, memory)?;
        // Keep the net IRQ asserted while RX frames are staged but the guest
        // has not posted buffers yet. Clearing it left auth27 with synack:1
        // on the host and a curl connect timeout in the guest.
        let net_rx_pending = self.net.rx_pending();
        if net_completed != 0 || net_rx_pending || self.net_transport.read(0x060)? != 0 {
            if net_rx_pending && net_completed == 0 {
                let _ = self.net_transport.force_used_interrupt();
            }
            self.pending_irqs.insert(NET_IRQ);
        } else {
            self.pending_irqs.remove(&NET_IRQ);
        }
        Ok(block_completed + console_completed + vsock_completed + net_completed)
    }
    pub(crate) fn listen_vsock(
        &mut self,
        port: u32,
    ) -> Result<std::sync::mpsc::Receiver<crate::vsock::VsockConnection>, RelayError> {
        self.vsock.listen(port)
    }
    pub(crate) fn console(&self) -> &[u8] {
        &self.console
    }
    pub(crate) fn pending_irq(&self) -> Option<u32> {
        self.pending_irqs.iter().next().copied()
    }
    pub(crate) fn raise_irq(&mut self, irq: u32) {
        self.pending_irqs.insert(irq);
    }
    pub(crate) fn debug_activity(
        &self,
    ) -> (
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ) {
        let (console_rx_notifications, console_rx_completions) =
            self.console_transport.queue_activity(0);
        let (console_tx_notifications, console_tx_completions) =
            self.console_transport.queue_activity(1);
        (
            self.mmio_reads.get(),
            self.mmio_writes,
            self.pl011_writes,
            self.last_mmio_read.get(),
            self.last_mmio_write,
            self.gicc_iar_reads.get(),
            self.gicc_spurious_reads.get(),
            self.gicc_eoir_writes,
            self.last_irq_ack.get(),
            self.last_irq_eoi,
            console_rx_notifications,
            console_rx_completions,
            console_tx_notifications,
            console_tx_completions,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vsock_has_real_device_id_cid_and_three_queues() {
        let mut bus = Bus::default();
        let b = VIRTIO_VSOCK_BASE;
        assert!(bus.handles(b));
        assert_eq!(bus.read32(b + 8).unwrap(), 19);
        assert_eq!(bus.read32(b + 0x100).unwrap(), 3);
        assert_eq!(bus.read32(b + 0x104).unwrap(), 0);
        assert_eq!(bus.read32(b + 0xfc).unwrap(), 0);
        for queue in 0..3 {
            bus.write32(b + 0x30, queue).unwrap();
            assert_eq!(bus.read32(b + 0x34).unwrap(), 256);
        }
        bus.write32(b + 0x30, 3).unwrap();
        assert_eq!(bus.read32(b + 0x34).unwrap(), 0);
        bus.write32(b + 0x70, 0).unwrap();
        assert_eq!(bus.read32(b + 0x60).unwrap(), 0);
    }

    #[test]
    fn pl011_collects_early_console_without_aliasing_ram() {
        let mut bus = Bus::default();
        bus.write32(PL011_BASE, b'R' as u32).unwrap();
        bus.write32(PL011_BASE, b'\n' as u32).unwrap();
        assert_eq!(bus.console(), b"R\n");
        assert_eq!(bus.read32(PL011_BASE + UART_FR).unwrap(), 0);
        assert!(bus.write32(PL011_BASE - 4, 0).is_err());
    }

    #[test]
    fn software_interrupt_filters_deliver_and_complete_on_cpu_zero() {
        for filter in 0..4 {
            for targets in [0, 1, 2, 255] {
                for irq in 0..16 {
                    let mut bus = Bus::default();
                    bus.write32(GICD_BASE + 0xf00, (filter << 24) | (targets << 16) | irq)
                        .unwrap();
                    let expected = if filter == 2 || (filter == 0 && targets & 1 != 0) {
                        irq
                    } else {
                        1023
                    };
                    assert_eq!(bus.read32(GICC_BASE + 0x00c).unwrap(), expected);
                    bus.write32(GICC_BASE + 0x010, expected).unwrap();
                    assert_eq!(bus.read32(GICC_BASE + 0x00c).unwrap(), 1023);
                }
            }
        }
    }

    #[test]
    fn gic_cpu_interface_acknowledges_and_completes_pending_irq() {
        let mut bus = Bus::default();
        bus.raise_irq(27);
        assert_eq!(bus.read32(GICC_BASE + 0x00c).unwrap(), 27);
        bus.write32(GICC_BASE + 0x010, 27).unwrap();
        assert_eq!(bus.read32(GICC_BASE + 0x00c).unwrap(), 1023);
        assert_eq!(bus.read16(GICD_BASE + 0x842), Some(0x0101));
    }

    #[test]
    fn gic_acknowledges_virtio_net_before_the_timer() {
        let mut bus = Bus::default();
        bus.raise_irq(27);
        bus.raise_irq(NET_IRQ);
        assert_eq!(bus.read32(GICC_BASE + 0x00c).unwrap(), NET_IRQ);
        bus.write32(GICC_BASE + 0x010, NET_IRQ).unwrap();
        assert_eq!(bus.read32(GICC_BASE + 0x00c).unwrap(), 27);
    }

    #[test]
    fn net_status_halfword_is_link_up() {
        let mut bus = Bus::default();
        let status = VIRTIO_NET_BASE + 0x106;
        assert!(bus.handles(status));
        assert_eq!(bus.read8(VIRTIO_NET_BASE + 0x100), Some(0x52));
        assert_eq!(bus.read16(status), Some(1));
        assert!(bus.write16(status, 0));
    }
}

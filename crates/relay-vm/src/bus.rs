//! Fixed MMIO windows. RAM remains solely in GuestMemory.
use relay_core::RelayError;
use std::path::Path;

pub(crate) const PL011_BASE: u64 = 0x0900_0000;
const PL011_SIZE: u64 = 0x1000;
pub(crate) const VIRTIO_BLOCK_BASE: u64 = 0x0a00_0000;
const VIRTIO_MMIO_SIZE: u64 = 0x1000;
const UART_DR: u64 = 0;
const UART_FR: u64 = 0x18;
pub(crate) struct Bus {
    console: Vec<u8>,
    block_transport: Option<crate::virtio_mmio::Transport>,
    block: Option<crate::virtio_block::BlockDevice>,
}
impl Default for Bus {
    fn default() -> Self {
        Self {
            console: Vec::new(),
            block_transport: None,
            block: None,
        }
    }
}
impl Bus {
    pub(crate) fn with_block(path: &Path) -> Result<Self, RelayError> {
        Ok(Self {
            console: Vec::new(),
            block_transport: Some(crate::virtio_mmio::Transport::new(2, 1 << 32, 256)),
            block: Some(crate::virtio_block::BlockDevice::from_file(path, 4)?),
        })
    }
    pub(crate) fn handles(&self, address: u64) -> bool {
        (PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address)
            || (self.block_transport.is_some()
                && (VIRTIO_BLOCK_BASE..VIRTIO_BLOCK_BASE + VIRTIO_MMIO_SIZE).contains(&address))
    }
    pub(crate) fn read32(&self, address: u64) -> Result<u32, RelayError> {
        if address & 3 != 0 {
            return Err(RelayError::Failed(format!(
                "StaticCpu unmapped MMIO read {address:#x}"
            )));
        }
        if (PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address) {
            return match address - PL011_BASE {
                UART_FR | UART_DR => Ok(0),
                _ => Ok(0),
            };
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
        if (PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address) {
            if address - PL011_BASE == UART_DR {
                self.console.push(value as u8);
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
        Err(RelayError::Failed(format!(
            "StaticCpu unmapped MMIO write {address:#x}"
        )))
    }
    pub(crate) fn service(
        &mut self,
        memory: &mut crate::guest::GuestMemory,
    ) -> Result<usize, RelayError> {
        match (&mut self.block, &mut self.block_transport) {
            (Some(block), Some(transport)) => block.process_notified_queue(transport, memory),
            _ => Ok(0),
        }
    }
    pub(crate) fn console(&self) -> &[u8] {
        &self.console
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pl011_collects_early_console_without_aliasing_ram() {
        let mut bus = Bus::default();
        bus.write32(PL011_BASE, b'R' as u32).unwrap();
        bus.write32(PL011_BASE, b'\n' as u32).unwrap();
        assert_eq!(bus.console(), b"R\n");
        assert_eq!(bus.read32(PL011_BASE + UART_FR).unwrap(), 0);
        assert!(bus.write32(PL011_BASE - 4, 0).is_err());
    }
}

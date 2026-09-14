//! Fixed MMIO windows. RAM remains solely in GuestMemory.
use relay_core::RelayError;
pub(crate) const PL011_BASE: u64 = 0x0900_0000;
const PL011_SIZE: u64 = 0x1000;
const UART_DR: u64 = 0;
const UART_FR: u64 = 0x18;
#[derive(Debug, Default)]
pub(crate) struct Bus {
    console: Vec<u8>,
}
impl Bus {
    pub(crate) fn read32(&self, address: u64) -> Result<u32, RelayError> {
        if !(PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address) || address & 3 != 0 {
            return Err(RelayError::Failed(format!(
                "StaticCpu unmapped MMIO read {address:#x}"
            )));
        }
        match address - PL011_BASE {
            UART_FR | UART_DR => Ok(0),
            _ => Ok(0),
        }
    }
    pub(crate) fn write32(&mut self, address: u64, value: u32) -> Result<(), RelayError> {
        if !(PL011_BASE..PL011_BASE + PL011_SIZE).contains(&address) || address & 3 != 0 {
            return Err(RelayError::Failed(format!(
                "StaticCpu unmapped MMIO write {address:#x}"
            )));
        }
        if address - PL011_BASE == UART_DR {
            self.console.push(value as u8);
        }
        Ok(())
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

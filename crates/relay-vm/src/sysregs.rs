//! EL1 system-register state exposed by the StaticCpu interpreter.
use relay_core::RelayError;

pub(crate) const SCTLR_EL1: u16 = 0xc080;
pub(crate) const TTBR0_EL1: u16 = 0xc100;
pub(crate) const TCR_EL1: u16 = 0xc102;
pub(crate) const MAIR_EL1: u16 = 0xc510;
pub(crate) const VBAR_EL1: u16 = 0xc600;
pub(crate) const CNTFRQ_EL0: u16 = 0xdf00;
pub(crate) const CNTPCT_EL0: u16 = 0xdf01;
pub(crate) const MIDR_EL1: u16 = 0xc000;
pub(crate) const ID_AA64PFR0_EL1: u16 = 0xc020;
pub(crate) const CURRENT_EL: u16 = 0xc212;

#[derive(Debug, Clone)]
pub(crate) struct SysRegs {
    pub sctlr_el1: u64,
    pub ttbr0_el1: u64,
    pub tcr_el1: u64,
    pub mair_el1: u64,
    pub vbar_el1: u64,
    counter: u64,
    counter_frequency: u64,
}
impl SysRegs {
    pub(crate) fn new(counter_frequency: u64) -> Result<Self, RelayError> {
        if counter_frequency == 0 {
            return Err(RelayError::Failed("CNTFRQ must be non-zero".into()));
        }
        Ok(Self {
            sctlr_el1: 0,
            ttbr0_el1: 0,
            tcr_el1: 0,
            mair_el1: 0,
            vbar_el1: 0,
            counter: 0,
            counter_frequency,
        })
    }
    pub(crate) fn tick(&mut self, ticks: u64) {
        self.counter = self.counter.wrapping_add(ticks);
    }
    pub(crate) fn read(&self, reg: u16) -> Result<u64, RelayError> {
        match reg {
            SCTLR_EL1 => Ok(self.sctlr_el1),
            TTBR0_EL1 => Ok(self.ttbr0_el1),
            TCR_EL1 => Ok(self.tcr_el1),
            MAIR_EL1 => Ok(self.mair_el1),
            VBAR_EL1 => Ok(self.vbar_el1),
            CNTFRQ_EL0 => Ok(self.counter_frequency),
            CNTPCT_EL0 => Ok(self.counter),
            MIDR_EL1 => Ok(0x410f_d0c0),
            ID_AA64PFR0_EL1 => Ok(0x11),
            CURRENT_EL => Ok(1 << 2),
            _ => Err(RelayError::Failed(format!(
                "StaticCpu unimplemented system register {reg:#06x}"
            ))),
        }
    }
    pub(crate) fn write(&mut self, reg: u16, value: u64) -> Result<(), RelayError> {
        match reg {
            SCTLR_EL1 => self.sctlr_el1 = value,
            TTBR0_EL1 => self.ttbr0_el1 = value,
            TCR_EL1 => self.tcr_el1 = value,
            MAIR_EL1 => self.mair_el1 = value,
            VBAR_EL1 => self.vbar_el1 = value,
            CNTFRQ_EL0 | CNTPCT_EL0 | MIDR_EL1 | ID_AA64PFR0_EL1 | CURRENT_EL => {
                return Err(RelayError::Failed(
                    "StaticCpu attempted write to read-only system register".into(),
                ))
            }
            _ => {
                return Err(RelayError::Failed(format!(
                    "StaticCpu unimplemented system register {reg:#06x}"
                )))
            }
        };
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn el1_registers_preserve_mmu_state_and_counter_is_monotonic() {
        let mut regs = SysRegs::new(24_000_000).unwrap();
        regs.write(TTBR0_EL1, 0x4000).unwrap();
        regs.write(SCTLR_EL1, 1).unwrap();
        regs.tick(12);
        assert_eq!(regs.read(TTBR0_EL1).unwrap(), 0x4000);
        assert_eq!(regs.read(SCTLR_EL1).unwrap() & 1, 1);
        assert_eq!(regs.read(CNTFRQ_EL0).unwrap(), 24_000_000);
        assert_eq!(regs.read(CNTPCT_EL0).unwrap(), 12);
        assert_eq!(regs.read(CURRENT_EL).unwrap(), 4);
        assert!(regs.write(CNTFRQ_EL0, 1).is_err());
    }
}

//! EL1 system-register state exposed by the StaticCpu interpreter.
use relay_core::RelayError;

pub(crate) const SCTLR_EL1: u16 = 0xc080;
pub(crate) const CPACR_EL1: u16 = 0xc082;
pub(crate) const TTBR0_EL1: u16 = 0xc100;
pub(crate) const TTBR1_EL1: u16 = 0xc101;
pub(crate) const TCR_EL1: u16 = 0xc102;
pub(crate) const MAIR_EL1: u16 = 0xc510;
pub(crate) const VBAR_EL1: u16 = 0xc600;
pub(crate) const CNTFRQ_EL0: u16 = 0xdf00;
pub(crate) const CNTPCT_EL0: u16 = 0xdf01;
pub(crate) const MIDR_EL1: u16 = 0xc000;
pub(crate) const ID_AA64PFR0_EL1: u16 = 0xc020;
pub(crate) const ID_AA64DFR0_EL1: u16 = 0xc028;
pub(crate) const ID_AA64ISAR0_EL1: u16 = 0xc030;
pub(crate) const ID_AA64ISAR1_EL1: u16 = 0xc031;
pub(crate) const ID_AA64MMFR0_EL1: u16 = 0xc038;
pub(crate) const ID_AA64MMFR1_EL1: u16 = 0xc039;
pub(crate) const ID_AA64MMFR2_EL1: u16 = 0xc03a;
pub(crate) const MDSCR_EL1: u16 = 0x8012;
pub(crate) const CURRENT_EL: u16 = 0xc212;
pub(crate) const SPSR_EL1: u16 = 0xc200;
pub(crate) const ELR_EL1: u16 = 0xc201;
pub(crate) const SPSEL: u16 = 0xc210;
pub(crate) const CTR_EL0: u16 = 0xd801;
pub(crate) const DCZID_EL0: u16 = 0xd807;

#[derive(Debug, Clone)]
pub(crate) struct SysRegs {
    pub sctlr_el1: u64,
    pub cpacr_el1: u64,
    pub ttbr0_el1: u64,
    pub ttbr1_el1: u64,
    pub tcr_el1: u64,
    pub mair_el1: u64,
    pub vbar_el1: u64,
    pub spsr_el1: u64,
    pub elr_el1: u64,
    pub mdscr_el1: u64,
    spsel: u64,
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
            cpacr_el1: 0,
            ttbr0_el1: 0,
            ttbr1_el1: 0,
            tcr_el1: 0,
            mair_el1: 0,
            vbar_el1: 0,
            spsr_el1: 0,
            elr_el1: 0,
            mdscr_el1: 0,
            spsel: 1,
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
            CPACR_EL1 => Ok(self.cpacr_el1),
            TTBR0_EL1 => Ok(self.ttbr0_el1),
            TTBR1_EL1 => Ok(self.ttbr1_el1),
            TCR_EL1 => Ok(self.tcr_el1),
            MAIR_EL1 => Ok(self.mair_el1),
            VBAR_EL1 => Ok(self.vbar_el1),
            CNTFRQ_EL0 => Ok(self.counter_frequency),
            CNTPCT_EL0 => Ok(self.counter),
            MIDR_EL1 => Ok(0x410f_d0c0),
            ID_AA64PFR0_EL1 => Ok(0x11),
            ID_AA64DFR0_EL1 => Ok(0),
            ID_AA64ISAR0_EL1 | ID_AA64ISAR1_EL1 => Ok(0),
            // 48-bit physical addresses; baseline 4 KiB translation granule.
            ID_AA64MMFR0_EL1 => Ok(5),
            ID_AA64MMFR1_EL1 | ID_AA64MMFR2_EL1 => Ok(0),
            MDSCR_EL1 => Ok(self.mdscr_el1),
            CURRENT_EL => Ok(1 << 2),
            SPSR_EL1 => Ok(self.spsr_el1),
            ELR_EL1 => Ok(self.elr_el1),
            SPSEL => Ok(self.spsel),
            // Cortex-A57-compatible 64-byte I/D cache lines. Relay executes
            // cache maintenance synchronously but Linux still requires
            // architecturally coherent geometry during early boot.
            CTR_EL0 => Ok(0x8444_c004),
            DCZID_EL0 => Ok(4),
            _ => Err(RelayError::Failed(format!(
                "StaticCpu unimplemented system register {reg:#06x}"
            ))),
        }
    }
    pub(crate) fn write(&mut self, reg: u16, value: u64) -> Result<(), RelayError> {
        match reg {
            SCTLR_EL1 => self.sctlr_el1 = value,
            CPACR_EL1 => self.cpacr_el1 = value,
            TTBR0_EL1 => self.ttbr0_el1 = value,
            TTBR1_EL1 => self.ttbr1_el1 = value,
            TCR_EL1 => self.tcr_el1 = value,
            MAIR_EL1 => self.mair_el1 = value,
            VBAR_EL1 => self.vbar_el1 = value,
            SPSR_EL1 => self.spsr_el1 = value,
            ELR_EL1 => self.elr_el1 = value,
            MDSCR_EL1 => self.mdscr_el1 = value,
            SPSEL if value <= 1 => self.spsel = value,
            SPSEL => {
                return Err(RelayError::Failed(
                    "StaticCpu SPSel value is invalid".into(),
                ))
            }
            CNTFRQ_EL0 | CNTPCT_EL0 | MIDR_EL1 | ID_AA64PFR0_EL1 | ID_AA64DFR0_EL1
            | ID_AA64ISAR0_EL1 | ID_AA64ISAR1_EL1 | ID_AA64MMFR0_EL1 | ID_AA64MMFR1_EL1
            | ID_AA64MMFR2_EL1 | CURRENT_EL | CTR_EL0 | DCZID_EL0 => {
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
        regs.write(TTBR1_EL1, 0x8000).unwrap();
        regs.write(SCTLR_EL1, 1).unwrap();
        regs.tick(12);
        assert_eq!(regs.read(TTBR0_EL1).unwrap(), 0x4000);
        assert_eq!(regs.read(TTBR1_EL1).unwrap(), 0x8000);
        assert_eq!(regs.read(SCTLR_EL1).unwrap() & 1, 1);
        assert_eq!(regs.read(CNTFRQ_EL0).unwrap(), 24_000_000);
        assert_eq!(regs.read(CNTPCT_EL0).unwrap(), 12);
        assert_eq!(regs.read(CURRENT_EL).unwrap(), 4);
        assert_eq!(regs.read(SPSEL).unwrap(), 1);
        regs.write(SPSR_EL1, 0x3c5).unwrap();
        regs.write(ELR_EL1, 0x80000).unwrap();
        assert_eq!(regs.read(SPSR_EL1).unwrap(), 0x3c5);
        assert_eq!(regs.read(ELR_EL1).unwrap(), 0x80000);
        regs.write(SPSEL, 0).unwrap();
        assert_eq!(regs.read(SPSEL).unwrap(), 0);
        assert_eq!(regs.read(CTR_EL0).unwrap(), 0x8444_c004);
        assert_eq!(regs.read(DCZID_EL0).unwrap(), 4);
        assert_eq!(regs.read(ID_AA64MMFR0_EL1).unwrap() & 7, 5);
        assert!(regs.write(CNTFRQ_EL0, 1).is_err());
    }
}

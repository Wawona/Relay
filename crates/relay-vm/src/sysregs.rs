//! EL1 system-register state exposed by the StaticCpu interpreter.
use relay_core::RelayError;

pub(crate) const SCTLR_EL1: u16 = 0xc080;
pub(crate) const CPACR_EL1: u16 = 0xc082;
pub(crate) const TTBR0_EL1: u16 = 0xc100;
pub(crate) const TTBR1_EL1: u16 = 0xc101;
pub(crate) const TCR_EL1: u16 = 0xc102;
pub(crate) const MAIR_EL1: u16 = 0xc510;
pub(crate) const VBAR_EL1: u16 = 0xc600;
pub(crate) const TPIDR_EL1: u16 = 0xc684;
pub(crate) const TPIDR_EL0: u16 = 0xde82;
pub(crate) const TPIDRRO_EL0: u16 = 0xde83;
pub(crate) const CONTEXTIDR_EL1: u16 = 0xc681;
pub(crate) const CNTFRQ_EL0: u16 = 0xdf00;
pub(crate) const CNTKCTL_EL1: u16 = 0xc708;
pub(crate) const CNTPCT_EL0: u16 = 0xdf01;
pub(crate) const CNTP_TVAL_EL0: u16 = 0xdf10;
pub(crate) const CNTP_CTL_EL0: u16 = 0xdf11;
pub(crate) const CNTP_CVAL_EL0: u16 = 0xdf12;
pub(crate) const CNTVCT_EL0: u16 = 0xdf02;
pub(crate) const CNTV_TVAL_EL0: u16 = 0xdf18;
pub(crate) const CNTV_CTL_EL0: u16 = 0xdf19;
pub(crate) const CNTV_CVAL_EL0: u16 = 0xdf1a;
pub(crate) const MIDR_EL1: u16 = 0xc000;
pub(crate) const MPIDR_EL1: u16 = 0xc005;
pub(crate) const REVIDR_EL1: u16 = 0xc006;
pub(crate) const ID_AA64PFR0_EL1: u16 = 0xc020;
pub(crate) const ID_AA64PFR1_EL1: u16 = 0xc021;
pub(crate) const ID_AA64PFR2_EL1: u16 = 0xc022;
pub(crate) const ID_AA64ZFR0_EL1: u16 = 0xc024;
pub(crate) const ID_AA64SMFR0_EL1: u16 = 0xc025;
pub(crate) const ID_AA64FPFR0_EL1: u16 = 0xc027;
pub(crate) const ID_AA64DFR0_EL1: u16 = 0xc028;
pub(crate) const ID_AA64DFR1_EL1: u16 = 0xc029;
pub(crate) const ID_AA64ISAR0_EL1: u16 = 0xc030;
pub(crate) const ID_AA64ISAR1_EL1: u16 = 0xc031;
pub(crate) const ID_AA64ISAR2_EL1: u16 = 0xc032;
pub(crate) const ID_AA64ISAR3_EL1: u16 = 0xc033;
pub(crate) const ID_AA64MMFR0_EL1: u16 = 0xc038;
pub(crate) const ID_AA64MMFR1_EL1: u16 = 0xc039;
pub(crate) const ID_AA64MMFR2_EL1: u16 = 0xc03a;
pub(crate) const ID_AA64MMFR3_EL1: u16 = 0xc03b;
pub(crate) const ID_AA64MMFR4_EL1: u16 = 0xc03c;
pub(crate) const GMID_EL1: u16 = 0xc804;
pub(crate) const SMIDR_EL1: u16 = 0xc806;
pub(crate) const MDSCR_EL1: u16 = 0x8012;
pub(crate) const OSDLR_EL1: u16 = 0x809c;
pub(crate) const OSLAR_EL1: u16 = 0x8084;
pub(crate) const DBGBCR0_EL1: u16 = 0x8005;
pub(crate) const DBGBVR0_EL1: u16 = 0x8004;
pub(crate) const DBGWVR0_EL1: u16 = 0x8006;
pub(crate) const DBGWCR0_EL1: u16 = 0x8007;
pub(crate) const CURRENT_EL: u16 = 0xc212;
pub(crate) const SPSR_EL1: u16 = 0xc200;
pub(crate) const ELR_EL1: u16 = 0xc201;
pub(crate) const ESR_EL1: u16 = 0xc290;
pub(crate) const FAR_EL1: u16 = 0xc300;
pub(crate) const SP_EL0: u16 = 0xc208;
pub(crate) const SPSEL: u16 = 0xc210;
pub(crate) const CLIDR_EL1: u16 = 0xc801;
pub(crate) const AIDR_EL1: u16 = 0xc807;
pub(crate) const CTR_EL0: u16 = 0xd801;
pub(crate) const DCZID_EL0: u16 = 0xd807;
pub(crate) const DAIF: u16 = 0xda11;
pub(crate) const FPCR: u16 = 0xda20;
pub(crate) const FPSR: u16 = 0xda21;

#[derive(Debug, Clone)]
pub(crate) struct SysRegs {
    pub sctlr_el1: u64,
    pub cpacr_el1: u64,
    pub ttbr0_el1: u64,
    pub ttbr1_el1: u64,
    pub tcr_el1: u64,
    pub mair_el1: u64,
    pub vbar_el1: u64,
    pub tpidr_el1: u64,
    pub tpidr_el0: u64,
    pub tpidrro_el0: u64,
    pub contextidr_el1: u64,
    pub cntkctl_el1: u64,
    pub spsr_el1: u64,
    pub elr_el1: u64,
    pub esr_el1: u64,
    pub far_el1: u64,
    pub sp_el0: u64,
    pub mdscr_el1: u64,
    pub osdlr_el1: u64,
    pub oslar_el1: u64,
    pub dbgbcr0_el1: u64,
    pub dbgbvr0_el1: u64,
    pub dbgwvr0_el1: u64,
    pub dbgwcr0_el1: u64,
    pub daif: u64,
    pub fpcr: u64,
    pub fpsr: u64,
    pub current_el: u8,
    pub(crate) spsel: u64,
    counter: u64,
    counter_frequency: u64,
    cntp_cval: u64,
    cntp_ctl: u64,
    cntv_cval: u64,
    cntv_ctl: u64,
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
            tpidr_el1: 0,
            tpidr_el0: 0,
            tpidrro_el0: 0,
            contextidr_el1: 0,
            cntkctl_el1: 0,
            spsr_el1: 0,
            elr_el1: 0,
            esr_el1: 0,
            far_el1: 0,
            sp_el0: 0,
            mdscr_el1: 0,
            osdlr_el1: 0,
            oslar_el1: 0,
            dbgbcr0_el1: 0,
            dbgbvr0_el1: 0,
            dbgwvr0_el1: 0,
            dbgwcr0_el1: 0,
            daif: 0x3c0,
            fpcr: 0,
            fpsr: 0,
            current_el: 1,
            spsel: 1,
            counter: 0,
            counter_frequency,
            cntp_cval: 0,
            cntp_ctl: 0,
            cntv_cval: 0,
            cntv_ctl: 0,
        })
    }
    pub(crate) fn tick(&mut self, ticks: u64) {
        self.counter = self.counter.wrapping_add(ticks);
    }
    fn timer_condition(&self, control: u64, compare: u64) -> bool {
        control & 1 != 0 && self.counter >= compare
    }
    fn physical_timer_pending(&self) -> bool {
        self.cntp_ctl & 2 == 0 && self.timer_condition(self.cntp_ctl, self.cntp_cval)
    }
    fn virtual_timer_pending(&self) -> bool {
        self.cntv_ctl & 2 == 0 && self.timer_condition(self.cntv_ctl, self.cntv_cval)
    }
    pub(crate) fn timer_pending(&self) -> bool {
        self.physical_timer_pending() || self.virtual_timer_pending()
    }
    pub(crate) fn pending_timer_irq(&self) -> Option<u32> {
        if self.virtual_timer_pending() {
            Some(27) // GIC PPI 11: arm,armv8-timer virtual timer
        } else if self.physical_timer_pending() {
            Some(30) // GIC PPI 14: arm,armv8-timer non-secure physical timer
        } else {
            None
        }
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
            TPIDR_EL1 => Ok(self.tpidr_el1),
            TPIDR_EL0 => Ok(self.tpidr_el0),
            TPIDRRO_EL0 => Ok(self.tpidrro_el0),
            CONTEXTIDR_EL1 => Ok(self.contextidr_el1),
            CNTKCTL_EL1 => Ok(self.cntkctl_el1),
            CNTFRQ_EL0 => Ok(self.counter_frequency),
            CNTPCT_EL0 => Ok(self.counter),
            CNTP_TVAL_EL0 => Ok(self.cntp_cval.wrapping_sub(self.counter) as u32 as u64),
            CNTP_CTL_EL0 => Ok(self.cntp_ctl
                | (u64::from(self.timer_condition(self.cntp_ctl, self.cntp_cval)) << 2)),
            CNTP_CVAL_EL0 => Ok(self.cntp_cval),
            CNTVCT_EL0 => Ok(self.counter),
            CNTV_TVAL_EL0 => Ok(self.cntv_cval.wrapping_sub(self.counter) as u32 as u64),
            CNTV_CTL_EL0 => Ok(self.cntv_ctl
                | (u64::from(self.timer_condition(self.cntv_ctl, self.cntv_cval)) << 2)),
            CNTV_CVAL_EL0 => Ok(self.cntv_cval),
            MIDR_EL1 => Ok(0x410f_d0c0),
            MPIDR_EL1 => Ok(0x8000_0000),
            REVIDR_EL1 => Ok(0),
            ID_AA64PFR0_EL1 => Ok(0x11),
            ID_AA64PFR1_EL1 | ID_AA64PFR2_EL1 | ID_AA64ZFR0_EL1 | ID_AA64SMFR0_EL1
            | ID_AA64FPFR0_EL1 | ID_AA64DFR0_EL1 | ID_AA64DFR1_EL1 | ID_AA64ISAR0_EL1
            | ID_AA64ISAR1_EL1 | ID_AA64ISAR2_EL1 | ID_AA64ISAR3_EL1 => Ok(0),
            // 48-bit PA; TGran4=0 and TGran16=1 advertise our two
            // translation granules. TGran64=15 rejects the unimplemented
            // 64 KiB geometry. TGran16=0 makes Linux park before MMU enable.
            ID_AA64MMFR0_EL1 => Ok(5 | (1 << 20) | (0xf << 24)),
            ID_AA64MMFR1_EL1 | ID_AA64MMFR2_EL1 | ID_AA64MMFR3_EL1 | ID_AA64MMFR4_EL1
            | GMID_EL1 | SMIDR_EL1 => Ok(0),
            MDSCR_EL1 => Ok(self.mdscr_el1),
            OSDLR_EL1 => Ok(self.osdlr_el1),
            OSLAR_EL1 => Ok(self.oslar_el1),
            DBGBCR0_EL1 => Ok(self.dbgbcr0_el1),
            DBGBVR0_EL1 => Ok(self.dbgbvr0_el1),
            DBGWVR0_EL1 => Ok(self.dbgwvr0_el1),
            DBGWCR0_EL1 => Ok(self.dbgwcr0_el1),
            CURRENT_EL => Ok(u64::from(self.current_el) << 2),
            SPSR_EL1 => Ok(self.spsr_el1),
            ELR_EL1 => Ok(self.elr_el1),
            ESR_EL1 => Ok(self.esr_el1),
            FAR_EL1 => Ok(self.far_el1),
            SP_EL0 => Ok(self.sp_el0),
            SPSEL => Ok(self.spsel),
            // StaticCpu memory is coherent and has no guest-visible set/way
            // cache hierarchy. Linux therefore has no levels to enumerate.
            CLIDR_EL1 => Ok(0),
            AIDR_EL1 => Ok(0),
            // Cortex-A57-compatible 64-byte I/D cache lines. Relay executes
            // cache maintenance synchronously but Linux still requires
            // architecturally coherent geometry during early boot.
            CTR_EL0 => Ok(0x8444_c004),
            DCZID_EL0 => Ok(4),
            DAIF => Ok(self.daif),
            FPCR => Ok(self.fpcr),
            FPSR => Ok(self.fpsr),
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
            TPIDR_EL1 => self.tpidr_el1 = value,
            TPIDR_EL0 => self.tpidr_el0 = value,
            TPIDRRO_EL0 => self.tpidrro_el0 = value,
            CONTEXTIDR_EL1 => self.contextidr_el1 = value,
            CNTKCTL_EL1 => self.cntkctl_el1 = value,
            SPSR_EL1 => self.spsr_el1 = value,
            ELR_EL1 => self.elr_el1 = value,
            ESR_EL1 => self.esr_el1 = value,
            FAR_EL1 => self.far_el1 = value,
            SP_EL0 => self.sp_el0 = value,
            MDSCR_EL1 => self.mdscr_el1 = value,
            OSDLR_EL1 => self.osdlr_el1 = value,
            OSLAR_EL1 => self.oslar_el1 = value,
            DBGBCR0_EL1 => self.dbgbcr0_el1 = value,
            DBGBVR0_EL1 => self.dbgbvr0_el1 = value,
            DBGWVR0_EL1 => self.dbgwvr0_el1 = value,
            DBGWCR0_EL1 => self.dbgwcr0_el1 = value,
            DAIF => self.daif = value & 0x3c0,
            // Keep only architecturally defined control/status fields. StaticCpu
            // does not yet execute scalar FP arithmetic, but Linux and libc
            // initialize and restore this state while entering userspace.
            FPCR => self.fpcr = value & 0x07c0_0000,
            FPSR => self.fpsr = value & 0xf800_009f,
            CNTP_TVAL_EL0 => self.cntp_cval = self.counter.wrapping_add(value as i32 as i64 as u64),
            CNTP_CTL_EL0 => self.cntp_ctl = value & 3,
            CNTP_CVAL_EL0 => self.cntp_cval = value,
            CNTV_TVAL_EL0 => self.cntv_cval = self.counter.wrapping_add(value as i32 as i64 as u64),
            CNTV_CTL_EL0 => self.cntv_ctl = value & 3,
            CNTV_CVAL_EL0 => self.cntv_cval = value,
            SPSEL if value <= 1 => self.spsel = value,
            SPSEL => {
                return Err(RelayError::Failed(
                    "StaticCpu SPSel value is invalid".into(),
                ))
            }
            CNTFRQ_EL0 | CNTPCT_EL0 | CNTVCT_EL0 | MIDR_EL1 | MPIDR_EL1 | REVIDR_EL1
            | ID_AA64PFR0_EL1 | ID_AA64PFR1_EL1 | ID_AA64PFR2_EL1 | ID_AA64ZFR0_EL1
            | ID_AA64SMFR0_EL1 | ID_AA64FPFR0_EL1 | ID_AA64DFR0_EL1 | ID_AA64DFR1_EL1
            | ID_AA64ISAR0_EL1 | ID_AA64ISAR1_EL1 | ID_AA64ISAR2_EL1 | ID_AA64ISAR3_EL1
            | ID_AA64MMFR0_EL1 | ID_AA64MMFR1_EL1 | ID_AA64MMFR2_EL1 | ID_AA64MMFR3_EL1
            | ID_AA64MMFR4_EL1 | GMID_EL1 | SMIDR_EL1 | CURRENT_EL | CTR_EL0 | DCZID_EL0
            | CLIDR_EL1 | AIDR_EL1 => {
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
    fn timer_status_is_independent_of_interrupt_mask() {
        for (control, compare, irq) in [
            (CNTP_CTL_EL0, CNTP_CVAL_EL0, 30),
            (CNTV_CTL_EL0, CNTV_CVAL_EL0, 27),
        ] {
            let mut regs = SysRegs::new(24_000_000).unwrap();
            regs.write(compare, 12).unwrap();
            regs.write(control, 3).unwrap();
            assert_eq!(regs.read(control).unwrap(), 3);
            regs.tick(12);
            assert_eq!(regs.read(control).unwrap(), 7);
            assert_eq!(regs.pending_timer_irq(), None);
            regs.write(control, 1).unwrap();
            assert_eq!(regs.read(control).unwrap(), 5);
            assert_eq!(regs.pending_timer_irq(), Some(irq));
            regs.write(control, 0).unwrap();
            assert_eq!(regs.pending_timer_irq(), None);
        }
    }
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
        assert_eq!(regs.read(CNTVCT_EL0).unwrap(), 12);
        assert_eq!(regs.read(CURRENT_EL).unwrap(), 4);
        assert_eq!(regs.read(SPSEL).unwrap(), 1);
        regs.write(SPSR_EL1, 0x3c5).unwrap();
        regs.write(ELR_EL1, 0x80000).unwrap();
        regs.write(SP_EL0, 0x90000).unwrap();
        regs.write(TPIDR_EL1, 0xa0000).unwrap();
        regs.write(TPIDR_EL0, 0xb0000).unwrap();
        regs.write(TPIDRRO_EL0, 0xc0000).unwrap();
        regs.write(CONTEXTIDR_EL1, 42).unwrap();
        regs.write(OSDLR_EL1, 1).unwrap();
        regs.write(OSLAR_EL1, 1).unwrap();
        regs.write(DBGBCR0_EL1, 1).unwrap();
        regs.write(DBGBVR0_EL1, 2).unwrap();
        regs.write(DBGWVR0_EL1, 3).unwrap();
        regs.write(DBGWCR0_EL1, 4).unwrap();
        regs.write(CNTKCTL_EL1, 3).unwrap();
        regs.write(FPCR, u64::MAX).unwrap();
        regs.write(FPSR, u64::MAX).unwrap();
        assert_eq!(regs.read(SPSR_EL1).unwrap(), 0x3c5);
        assert_eq!(regs.read(ELR_EL1).unwrap(), 0x80000);
        assert_eq!(regs.read(SP_EL0).unwrap(), 0x90000);
        assert_eq!(regs.read(TPIDR_EL1).unwrap(), 0xa0000);
        assert_eq!(regs.read(TPIDR_EL0).unwrap(), 0xb0000);
        assert_eq!(regs.read(TPIDRRO_EL0).unwrap(), 0xc0000);
        assert_eq!(regs.read(CONTEXTIDR_EL1).unwrap(), 42);
        assert_eq!(regs.read(OSDLR_EL1).unwrap(), 1);
        assert_eq!(regs.read(OSLAR_EL1).unwrap(), 1);
        assert_eq!(regs.read(DBGBCR0_EL1).unwrap(), 1);
        assert_eq!(regs.read(DBGBVR0_EL1).unwrap(), 2);
        assert_eq!(regs.read(DBGWVR0_EL1).unwrap(), 3);
        assert_eq!(regs.read(DBGWCR0_EL1).unwrap(), 4);
        assert_eq!(regs.read(CNTKCTL_EL1).unwrap(), 3);
        assert_eq!(regs.read(FPCR).unwrap(), 0x07c0_0000);
        assert_eq!(regs.read(FPSR).unwrap(), 0xf800_009f);
        regs.write(SPSEL, 0).unwrap();
        assert_eq!(regs.read(SPSEL).unwrap(), 0);
        assert_eq!(regs.read(CTR_EL0).unwrap(), 0x8444_c004);
        assert_eq!(regs.read(DCZID_EL0).unwrap(), 4);
        assert_eq!(regs.read(CLIDR_EL1).unwrap(), 0);
        assert_eq!(regs.read(REVIDR_EL1).unwrap(), 0);
        assert_eq!(regs.read(AIDR_EL1).unwrap(), 0);
        let memory_features = regs.read(ID_AA64MMFR0_EL1).unwrap();
        assert_eq!(
            (memory_features >> 20) & 0xf,
            1,
            "16 KiB granule must be advertised"
        );
        assert_eq!(
            (memory_features >> 28) & 0xf,
            0,
            "4 KiB granule must remain supported"
        );
        assert_eq!(
            (memory_features >> 24) & 0xf,
            0xf,
            "64 KiB granule is not implemented"
        );
        assert_eq!(regs.read(ID_AA64MMFR0_EL1).unwrap() & 7, 5);
        assert!(regs.write(CNTFRQ_EL0, 1).is_err());
    }
}

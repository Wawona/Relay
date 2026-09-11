//! Minimal AArch64 EL1 synchronous exception state for Relay static CPU.

#![allow(dead_code)] // Wired into instruction execution during the next CPU pass.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExceptionClass {
    InstructionAbort,
    DataAbort,
    UndefinedInstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct El1State {
    pub vbar_el1: u64,
    pub elr_el1: u64,
    pub esr_el1: u32,
    pub spsr_el1: u64,
}

impl El1State {
    pub fn new(vbar_el1: u64) -> Self {
        Self {
            vbar_el1,
            elr_el1: 0,
            esr_el1: 0,
            spsr_el1: 0,
        }
    }
    /// Save fault context and return the EL1 synchronous vector. Linux may
    /// inspect ESR/ELR; Relay never silently continues after a fault.
    pub fn enter_sync(&mut self, class: ExceptionClass, fault_pc: u64, pstate: u64) -> u64 {
        self.elr_el1 = fault_pc;
        self.spsr_el1 = pstate;
        self.esr_el1 = match class {
            ExceptionClass::InstructionAbort => 0x8600_0000,
            ExceptionClass::DataAbort => 0x9600_0000,
            ExceptionClass::UndefinedInstruction => 0x0200_0000,
        };
        self.vbar_el1 + 0x200
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synchronous_abort_preserves_fault_context() {
        let mut el1 = El1State::new(0x4000);
        assert_eq!(
            el1.enter_sync(ExceptionClass::DataAbort, 0x8123, 0x3c5),
            0x4200
        );
        assert_eq!(el1.elr_el1, 0x8123);
        assert_eq!(el1.spsr_el1, 0x3c5);
        assert_eq!(el1.esr_el1, 0x9600_0000);
    }
}

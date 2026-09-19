//! Jitless, handler-shaped AArch64 EL1 core for guest boot.
//!
//! The CPU owns one GuestMemory arena and never allocates in `step`.  Decoder
//! misses are terminal and include the guest PC plus original instruction.

use crate::{
    bus::{Bus, PL011_BASE},
    guest::GuestMemory,
    sysregs::SysRegs,
};
use relay_core::RelayError;

pub(crate) struct StaticCpu {
    x: [u64; 31],
    sp: u64,
    nzcv: u8,
    pub(crate) pc: u64,
    memory: GuestMemory,
    bus: Bus,
    pub(crate) sysregs: SysRegs,
}

impl StaticCpu {
    pub(crate) fn new(memory: GuestMemory, entry_pc: u64) -> Result<Self, RelayError> {
        Ok(Self {
            x: [0; 31],
            sp: 0,
            nzcv: 0,
            pc: entry_pc,
            memory,
            bus: Bus::default(),
            sysregs: SysRegs::new(24_000_000)?,
        })
    }
    pub(crate) fn set_x(&mut self, register: u8, value: u64) {
        if register < 31 {
            self.x[register as usize] = value;
        }
    }
    fn x(&self, register: u32) -> u64 {
        if register == 31 {
            0
        } else {
            self.x[register as usize]
        }
    }
    fn set(&mut self, register: u32, value: u64) {
        if register != 31 {
            self.x[register as usize] = value;
        }
    }
    fn x_or_sp(&self, register: u32) -> u64 {
        if register == 31 {
            self.sp
        } else {
            self.x[register as usize]
        }
    }
    fn set_x_or_sp(&mut self, register: u32, value: u64) {
        if register == 31 {
            self.sp = value;
        } else {
            self.x[register as usize] = value;
        }
    }
    fn condition_holds(&self, condition: u32) -> bool {
        let n = self.nzcv & 8 != 0;
        let z = self.nzcv & 4 != 0;
        let c = self.nzcv & 2 != 0;
        let v = self.nzcv & 1 != 0;
        match condition {
            0 => z,
            1 => !z,
            2 => c,
            3 => !c,
            4 => n,
            5 => !n,
            6 => v,
            7 => !v,
            8 => c && !z,
            9 => !c || z,
            10 => n == v,
            11 => n != v,
            12 => !z && n == v,
            13 => z || n != v,
            14 => true,
            _ => false,
        }
    }
    fn word(&self) -> Result<u32, RelayError> {
        let mut bytes = [0; 4];
        self.memory.read(self.physical(self.pc)?, &mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }
    fn physical(&self, address: u64) -> Result<u64, RelayError> {
        if self.sysregs.sctlr_el1 & 1 == 0 {
            Ok(address)
        } else {
            let table = if address >> 63 != 0 {
                self.sysregs.ttbr1_el1
            } else {
                self.sysregs.ttbr0_el1
            };
            crate::mmu::walk_stage1(&self.memory, address, table, self.memory.page_size())
        }
    }
    fn read32(&self, address: u64) -> Result<u32, RelayError> {
        let physical = self.physical(address)?;
        if (PL011_BASE..PL011_BASE + 0x1000).contains(&physical) {
            return self.bus.read32(physical);
        }
        let mut bytes = [0; 4];
        self.memory.read(physical, &mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }
    fn write32(&mut self, address: u64, value: u32) -> Result<(), RelayError> {
        let physical = self.physical(address)?;
        if (PL011_BASE..PL011_BASE + 0x1000).contains(&physical) {
            return self.bus.write32(physical, value);
        }
        self.memory.write(physical, &value.to_le_bytes())
    }
    fn read8(&self, address: u64) -> Result<u8, RelayError> {
        let mut bytes = [0; 1];
        self.memory.read(self.physical(address)?, &mut bytes)?;
        Ok(bytes[0])
    }
    fn write8(&mut self, address: u64, value: u8) -> Result<(), RelayError> {
        self.memory.write(self.physical(address)?, &[value])
    }
    fn read16(&self, address: u64) -> Result<u16, RelayError> {
        let mut bytes = [0; 2];
        self.memory.read(self.physical(address)?, &mut bytes)?;
        Ok(u16::from_le_bytes(bytes))
    }
    fn write16(&mut self, address: u64, value: u16) -> Result<(), RelayError> {
        self.memory
            .write(self.physical(address)?, &value.to_le_bytes())
    }
    fn read64(&self, address: u64) -> Result<u64, RelayError> {
        let mut bytes = [0; 8];
        self.memory.read(self.physical(address)?, &mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }
    fn write64(&mut self, address: u64, value: u64) -> Result<(), RelayError> {
        self.memory
            .write(self.physical(address)?, &value.to_le_bytes())
    }

    pub(crate) fn run(&mut self, budget: u64) -> Result<(), RelayError> {
        for _ in 0..budget {
            self.step()?;
            self.sysregs.tick(1);
        }
        Err(RelayError::Failed(format!(
            "StaticCpu instruction budget exhausted at pc={:#x}",
            self.pc
        )))
    }
    pub(crate) fn step(&mut self) -> Result<(), RelayError> {
        let pc = self.pc;
        let insn = self
            .word()
            .map_err(|e| RelayError::Failed(format!("{e}; pc={pc:#x}")))?;
        let next = pc.wrapping_add(4);
        // Linux's ARM64 Image begins with the PE/COFF "MZ" signature encoded
        // as an architecturally harmless conditional compare. Treat only this
        // exact header instruction as a no-op before the branch at Image+4.
        if insn == 0xfa40_5a4d {
            self.pc = next;
            return Ok(());
        }
        // B/BL imm26.
        if insn & 0x7c00_0000 == 0x1400_0000 {
            let imm = sign_extend((insn & 0x03ff_ffff) as u64, 26) << 2;
            if insn & 0x8000_0000 != 0 {
                self.set(30, next);
            }
            self.pc = pc.wrapping_add(imm as u64);
            return Ok(());
        }
        // MRS/MSR (register).  The architectural system-register selector is
        // carried compactly so this handler cannot expose arbitrary host state.
        if insn & 0xfff0_0000 == 0xd530_0000 || insn & 0xfff0_0000 == 0xd510_0000 {
            let reg = (((insn >> 19) & 3) << 14
                | ((insn >> 16) & 7) << 11
                | ((insn >> 12) & 15) << 7
                | ((insn >> 8) & 15) << 3
                | ((insn >> 5) & 7)) as u16;
            let rt = insn & 31;
            if insn & 0x0020_0000 != 0 {
                let value = self.sysregs.read(reg).map_err(|error| {
                    RelayError::Failed(format!("{error}; pc={pc:#x} word={insn:#010x}"))
                })?;
                self.set(rt, value);
            } else {
                self.sysregs.write(reg, self.x(rt)).map_err(|error| {
                    RelayError::Failed(format!("{error}; pc={pc:#x} word={insn:#010x}"))
                })?;
            }
            self.pc = next;
            return Ok(());
        }
        // Immediate SPSel writes choose SP_EL0 or SP_EL1. Relay currently has
        // one physical stack register, but preserving the selector keeps
        // Linux's exception-entry state architecturally visible.
        if matches!(insn, 0xd500_40bf | 0xd500_41bf) {
            self.sysregs
                .write(crate::sysregs::SPSEL, u64::from(insn == 0xd500_41bf))?;
            self.pc = next;
            return Ok(());
        }
        // Exception return through the EL1 state Linux prepared. Interrupt
        // masks/mode remain guest state; instruction flow resumes at ELR_EL1.
        if insn == 0xd69f_03e0 {
            self.pc = self.sysregs.elr_el1;
            return Ok(());
        }
        // B.cond: the early boot path chiefly needs EQ/NE, but model all
        // integer condition encodings so compare loops remain deterministic.
        if insn & 0xff00_0010 == 0x5400_0000 {
            let imm = sign_extend(((insn >> 5) & 0x7ffff) as u64, 19) << 2;
            let take = self.condition_holds(insn & 15);
            self.pc = if take {
                pc.wrapping_add(imm as u64)
            } else {
                next
            };
            return Ok(());
        }
        // CBZ/CBNZ (32- and 64-bit forms).
        if insn & 0x7e00_0000 == 0x3400_0000 {
            let value = if insn & 0x8000_0000 != 0 {
                self.x(insn & 31)
            } else {
                self.x(insn & 31) as u32 as u64
            };
            let zero_branch = insn & (1 << 24) == 0;
            let take = (value == 0) == zero_branch;
            let imm = sign_extend(((insn >> 5) & 0x7ffff) as u64, 19) << 2;
            self.pc = if take {
                pc.wrapping_add(imm as u64)
            } else {
                next
            };
            return Ok(());
        }
        // ADR/ADRP.
        if insn & 0x1f00_0000 == 0x1000_0000 {
            let imm = sign_extend(
                ((((insn >> 5) & 0x7ffff) << 2) | ((insn >> 29) & 3)) as u64,
                21,
            );
            let base = if insn & 0x8000_0000 != 0 {
                pc & !0xfff
            } else {
                pc
            };
            let displacement = if insn & 0x8000_0000 != 0 {
                imm << 12
            } else {
                imm
            };
            self.set(insn & 31, base.wrapping_add(displacement as u64));
            self.pc = next;
            return Ok(());
        }
        // CSEL/CSINC/CSINV/CSNEG. Linux uses CSEL immediately after testing
        // the entry SCTLR state to retain or clear its boot-mode register.
        if insn & 0x1fe0_0000 == 0x1a80_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
            let value = if self.condition_holds((insn >> 12) & 15) {
                self.x((insn >> 5) & 31)
            } else {
                let alternate = self.x((insn >> 16) & 31);
                match ((insn >> 30) & 1, (insn >> 10) & 1) {
                    (0, 0) => alternate,
                    (0, 1) => alternate.wrapping_add(1),
                    (1, 0) => !alternate,
                    (1, 1) => alternate.wrapping_neg(),
                    _ => unreachable!(),
                }
            };
            self.set(insn & 31, value & mask);
            self.pc = next;
            return Ok(());
        }
        // CCMP/CCMN register and immediate. When the condition is false the
        // encoded NZCV literal is installed without evaluating operands.
        if insn & 0x1fe0_0000 == 0x1a40_0000 {
            if self.condition_holds((insn >> 12) & 15) {
                let is_64 = insn & 0x8000_0000 != 0;
                let width = if is_64 { 64 } else { 32 };
                let mask = low_mask(width);
                let left = self.x((insn >> 5) & 31) & mask;
                let right = if insn & (1 << 11) != 0 {
                    ((insn >> 16) & 31) as u64
                } else {
                    self.x((insn >> 16) & 31) & mask
                };
                let subtract = insn & (1 << 30) != 0;
                let result = if subtract {
                    left.wrapping_sub(right)
                } else {
                    left.wrapping_add(right)
                } & mask;
                let sign = width - 1;
                let n = result >> sign != 0;
                let z = result == 0;
                let c = if subtract {
                    left >= right
                } else {
                    u128::from(left) + u128::from(right) > u128::from(mask)
                };
                let v = if subtract {
                    (((left ^ right) & (left ^ result)) >> sign) != 0
                } else {
                    ((!(left ^ right) & (left ^ result)) >> sign) != 0
                };
                self.nzcv = (n as u8) << 3 | (z as u8) << 2 | (c as u8) << 1 | v as u8;
            } else {
                self.nzcv = (insn & 15) as u8;
            }
            self.pc = next;
            return Ok(());
        }
        // TBZ/TBNZ. The high bit of the tested bit index is also the
        // instruction's width selector.
        if insn & 0x7e00_0000 == 0x3600_0000 {
            let bit = ((insn >> 19) & 31) | ((insn >> 26) & 32);
            let value = self.x(insn & 31);
            let nonzero_branch = insn & (1 << 24) != 0;
            let take = (((value >> bit) & 1) != 0) == nonzero_branch;
            let imm = sign_extend(((insn >> 5) & 0x3fff) as u64, 14) << 2;
            self.pc = if take {
                pc.wrapping_add(imm as u64)
            } else {
                next
            };
            return Ok(());
        }
        // BR/BLR/RET. Linux uses register-indirect calls while the MMU is off.
        if matches!(insn & 0xffff_fc1f, 0xd61f_0000 | 0xd63f_0000 | 0xd65f_0000) {
            if insn & 0xffff_fc1f == 0xd63f_0000 {
                self.set(30, next);
            }
            self.pc = self.x((insn >> 5) & 31);
            return Ok(());
        }
        // ADD/SUB immediate, 32- and 64-bit forms. Register 31 denotes SP for
        // the source and for a non-flag-setting destination.
        if insn & 0x1f00_0000 == 0x1100_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let set_flags = insn & (1 << 29) != 0;
            let subtract = insn & (1 << 30) != 0;
            let shift = if insn & (1 << 22) != 0 { 12 } else { 0 };
            let immediate = (((insn >> 10) & 0xfff) as u64) << shift;
            let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
            let left = self.x_or_sp((insn >> 5) & 31) & mask;
            let value = if subtract {
                left.wrapping_sub(immediate) & mask
            } else {
                left.wrapping_add(immediate) & mask
            };
            if set_flags {
                self.set(insn & 31, value);
                let sign = if is_64 { 63 } else { 31 };
                let n = (value >> sign) != 0;
                let z = value == 0;
                let c = if subtract {
                    left >= immediate
                } else {
                    u128::from(left) + u128::from(immediate) > u128::from(mask)
                };
                let v = if subtract {
                    (((left ^ immediate) & (left ^ value)) >> sign) != 0
                } else {
                    ((!(left ^ immediate) & (left ^ value)) >> sign) != 0
                };
                self.nzcv = (n as u8) << 3 | (z as u8) << 2 | (c as u8) << 1 | v as u8;
            } else {
                self.set_x_or_sp(insn & 31, value);
            }
            self.pc = next;
            return Ok(());
        }
        // ADD/SUB shifted register, including CMP. Register 31 is XZR in this
        // encoding (unlike the immediate form, where it can denote SP).
        if insn & 0x1f20_0000 == 0x0b00_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let mask = low_mask(width);
            let amount = (insn >> 10) & 63;
            if !is_64 && amount >= 32 {
                return self.unsupported(pc, insn);
            }
            let source = self.x((insn >> 16) & 31) & mask;
            let right = match (insn >> 22) & 3 {
                0 => source << amount,
                1 => source >> amount,
                2 => {
                    if is_64 {
                        ((source as i64) >> amount) as u64
                    } else {
                        ((source as u32 as i32) >> amount) as u32 as u64
                    }
                }
                _ => return self.unsupported(pc, insn),
            } & mask;
            let left = self.x((insn >> 5) & 31) & mask;
            let subtract = insn & (1 << 30) != 0;
            let result = if subtract {
                left.wrapping_sub(right)
            } else {
                left.wrapping_add(right)
            } & mask;
            self.set(insn & 31, result);
            if insn & (1 << 29) != 0 {
                let sign = width - 1;
                let n = result >> sign != 0;
                let z = result == 0;
                let c = if subtract {
                    left >= right
                } else {
                    u128::from(left) + u128::from(right) > u128::from(mask)
                };
                let v = if subtract {
                    (((left ^ right) & (left ^ result)) >> sign) != 0
                } else {
                    ((!(left ^ right) & (left ^ result)) >> sign) != 0
                };
                self.nzcv = (n as u8) << 3 | (z as u8) << 2 | (c as u8) << 1 | v as u8;
            }
            self.pc = next;
            return Ok(());
        }
        // ADD/SUB extended register. Linux commonly folds a signed 32-bit
        // index into a 64-bit kernel pointer with the SXTW form.
        if insn & 0x1fe0_0000 == 0x0b20_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let mask = low_mask(width);
            let amount = (insn >> 10) & 7;
            if amount > 4 {
                return self.unsupported(pc, insn);
            }
            let option = (insn >> 13) & 7;
            let source = self.x((insn >> 16) & 31);
            let extended = match option {
                0 => source as u8 as u64,
                1 => source as u16 as u64,
                2 => source as u32 as u64,
                3 if is_64 => source,
                4 => source as u8 as i8 as i64 as u64,
                5 => source as u16 as i16 as i64 as u64,
                6 => source as u32 as i32 as i64 as u64,
                7 if is_64 => source as i64 as u64,
                _ => return self.unsupported(pc, insn),
            };
            let right = extended.wrapping_shl(amount) & mask;
            let left = self.x_or_sp((insn >> 5) & 31) & mask;
            let subtract = insn & (1 << 30) != 0;
            let set_flags = insn & (1 << 29) != 0;
            let result = if subtract {
                left.wrapping_sub(right)
            } else {
                left.wrapping_add(right)
            } & mask;
            if set_flags {
                self.set(insn & 31, result);
                let sign = width - 1;
                let n = result >> sign != 0;
                let z = result == 0;
                let c = if subtract {
                    left >= right
                } else {
                    u128::from(left) + u128::from(right) > u128::from(mask)
                };
                let v = if subtract {
                    (((left ^ right) & (left ^ result)) >> sign) != 0
                } else {
                    ((!(left ^ right) & (left ^ result)) >> sign) != 0
                };
                self.nzcv = (n as u8) << 3 | (z as u8) << 2 | (c as u8) << 1 | v as u8;
            } else {
                self.set_x_or_sp(insn & 31, result);
            }
            self.pc = next;
            return Ok(());
        }
        // AND/ORR/EOR/ANDS immediate. Linux uses the TST alias here while
        // selecting its early exception-level and cache path.
        if insn & 0x1f80_0000 == 0x1200_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let immediate = decode_logical_immediate(
                is_64,
                (insn >> 22) & 1,
                (insn >> 16) & 63,
                (insn >> 10) & 63,
            )
            .ok_or_else(|| {
                RelayError::Failed(format!(
                    "StaticCpu invalid logical immediate pc={pc:#x} word={insn:#010x}"
                ))
            })?;
            let left = self.x((insn >> 5) & 31);
            let result = match (insn >> 29) & 3 {
                0 | 3 => left & immediate,
                1 => left | immediate,
                2 => left ^ immediate,
                _ => unreachable!(),
            } & if is_64 { u64::MAX } else { u32::MAX as u64 };
            self.set(insn & 31, result);
            if (insn >> 29) & 3 == 3 {
                self.nzcv = ((result >> (width - 1)) as u8) << 3 | ((result == 0) as u8) << 2;
            }
            self.pc = next;
            return Ok(());
        }
        // AND/ORR/EOR shifted register. MOV aliases are ORR with XZR.
        if insn & 0x1f00_0000 == 0x0a00_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let amount = (insn >> 10) & 63;
            if !is_64 && amount >= 32 {
                return self.unsupported(pc, insn);
            }
            let left = self.x((insn >> 5) & 31);
            let source = self.x((insn >> 16) & 31);
            let mut right = match (insn >> 22) & 3 {
                0 => source << amount,
                1 => source >> amount,
                2 => ((source as i64) >> amount) as u64,
                _ => return self.unsupported(pc, insn),
            };
            if insn & (1 << 21) != 0 {
                right = !right;
            }
            let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
            let result = match (insn >> 29) & 3 {
                0 | 3 => left & right,
                1 => left | right,
                2 => left ^ right,
                _ => unreachable!(),
            } & mask;
            self.set(insn & 31, result);
            if (insn >> 29) & 3 == 3 {
                self.nzcv = ((result >> (width - 1)) as u8) << 3 | ((result == 0) as u8) << 2;
            }
            self.pc = next;
            return Ok(());
        }
        // LSLV/LSRV/ASRV/RORV. Linux derives cache-line byte counts with a
        // register shift after extracting CTR_EL0 fields.
        if insn & 0x1fe0_f000 == 0x1ac0_2000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let amount = (self.x((insn >> 16) & 31) & u64::from(width - 1)) as u32;
            let source = self.x((insn >> 5) & 31) & low_mask(width);
            let result = match (insn >> 10) & 3 {
                0 => source << amount,
                1 => source >> amount,
                2 => {
                    if is_64 {
                        ((source as i64) >> amount) as u64
                    } else {
                        ((source as u32 as i32) >> amount) as u32 as u64
                    }
                }
                3 => {
                    if is_64 {
                        source.rotate_right(amount)
                    } else {
                        (source as u32).rotate_right(amount) as u64
                    }
                }
                _ => unreachable!(),
            };
            self.set(insn & 31, result & low_mask(width));
            self.pc = next;
            return Ok(());
        }
        // RBIT/REV*/CLZ/CLS unary data processing.
        if insn & 0x7fff_c000 == 0x5ac0_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let source = self.x((insn >> 5) & 31) & low_mask(width);
            let result = match (insn >> 10) & 15 {
                0 => {
                    if is_64 {
                        source.reverse_bits()
                    } else {
                        (source as u32).reverse_bits() as u64
                    }
                }
                1 => reverse_bytes_in_halfwords(source, width),
                2 => {
                    if is_64 {
                        u64::from((source as u32).swap_bytes())
                            | (u64::from(((source >> 32) as u32).swap_bytes()) << 32)
                    } else {
                        (source as u32).swap_bytes() as u64
                    }
                }
                3 if is_64 => source.swap_bytes(),
                4 => {
                    if is_64 {
                        u64::from(source.leading_zeros())
                    } else {
                        u64::from((source as u32).leading_zeros())
                    }
                }
                5 => {
                    let count = if is_64 {
                        if source >> 63 == 0 {
                            source.leading_zeros()
                        } else {
                            (!source).leading_zeros()
                        }
                    } else {
                        let value = source as u32;
                        if value >> 31 == 0 {
                            value.leading_zeros()
                        } else {
                            (!value).leading_zeros()
                        }
                    };
                    u64::from(count.saturating_sub(1))
                }
                _ => return self.unsupported(pc, insn),
            };
            self.set(insn & 31, result & low_mask(width));
            self.pc = next;
            return Ok(());
        }
        // MADD/MSUB and MUL/MNEG aliases.
        if insn & 0x1fe0_0000 == 0x1b00_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let mask = low_mask(width);
            let left = self.x((insn >> 5) & 31) & mask;
            let right = self.x((insn >> 16) & 31) & mask;
            let addend = self.x((insn >> 10) & 31) & mask;
            let product = left.wrapping_mul(right) & mask;
            let result = if insn & (1 << 15) == 0 {
                product.wrapping_add(addend)
            } else {
                addend.wrapping_sub(product)
            } & mask;
            self.set(insn & 31, result);
            self.pc = next;
            return Ok(());
        }
        // MOVN/MOVZ/MOVK, 32- and 64-bit forms.
        if insn & 0x1f80_0000 == 0x1280_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let rd = insn & 31;
            let shift = ((insn >> 21) & 3) * 16;
            if !is_64 && shift >= 32 {
                return self.unsupported(pc, insn);
            }
            let imm = (((insn >> 5) & 0xffff) as u64) << shift;
            let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
            match (insn >> 29) & 3 {
                0 => self.set(rd, (!imm) & mask),
                2 => self.set(rd, imm & mask),
                3 => {
                    let halfword_mask = 0xffffu64 << shift;
                    self.set(rd, ((self.x(rd) & !halfword_mask) | imm) & mask);
                }
                _ => return self.unsupported(pc, insn),
            }
            self.pc = next;
            return Ok(());
        }
        // SBFM/BFM/UBFM and their ASR/BFI/BFXIL/LSL/LSR/UBFX aliases.
        if insn & 0x1f80_0000 == 0x1300_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let width_mask = low_mask(width);
            let n = (insn >> 22) & 1;
            let immr = (insn >> 16) & 63;
            let imms = (insn >> 10) & 63;
            if n != u32::from(is_64) || immr >= width || imms >= width {
                return self.unsupported(pc, insn);
            }
            let rd = insn & 31;
            let source = self.x((insn >> 5) & 31) & width_mask;
            let result = if imms >= immr {
                let bits = imms - immr + 1;
                let field = (source >> immr) & low_mask(bits);
                match (insn >> 29) & 3 {
                    0 => sign_extend_width(field, bits, width),
                    1 => (self.x(rd) & !low_mask(bits)) | field,
                    2 => field,
                    _ => return self.unsupported(pc, insn),
                }
            } else {
                let bits = imms + 1;
                let lsb = width - immr;
                let mask = low_mask(bits) << lsb;
                let field = (source << lsb) & mask;
                match (insn >> 29) & 3 {
                    0 => sign_extend_width(field, lsb + bits, width),
                    1 => (self.x(rd) & !mask) | field,
                    2 => field,
                    _ => return self.unsupported(pc, insn),
                }
            };
            self.set(rd, result & width_mask);
            self.pc = next;
            return Ok(());
        }
        // LDR literal, integer 32- and 64-bit forms.
        if matches!(insn & 0xff00_0000, 0x1800_0000 | 0x5800_0000) {
            let address =
                pc.wrapping_add((sign_extend(((insn >> 5) & 0x7ffff) as u64, 19) << 2) as u64);
            let value = if insn & 0x4000_0000 != 0 {
                self.read64(address)?
            } else {
                self.read32(address)? as u64
            };
            self.set(insn & 31, value);
            self.pc = next;
            return Ok(());
        }
        // Integer LDR/STR unsigned immediate, including byte/halfword and
        // sign-extending loads used after Linux establishes its early stack.
        if insn & 0x3b00_0000 == 0x3900_0000 {
            let size_shift = insn >> 30;
            let bytes = 1u64 << size_shift;
            let operation = (insn >> 22) & 3;
            let offset = ((insn >> 10) & 0xfff) as u64 * bytes;
            let address = self.x_or_sp((insn >> 5) & 31).wrapping_add(offset);
            let rt = insn & 31;
            match operation {
                0 => match bytes {
                    1 => self.write8(address, self.x(rt) as u8)?,
                    2 => self.write16(address, self.x(rt) as u16)?,
                    4 => self.write32(address, self.x(rt) as u32)?,
                    8 => self.write64(address, self.x(rt))?,
                    _ => unreachable!(),
                },
                1 => {
                    let value = match bytes {
                        1 => self.read8(address)? as u64,
                        2 => self.read16(address)? as u64,
                        4 => self.read32(address)? as u64,
                        8 => self.read64(address)?,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                2 if bytes < 8 => {
                    let value = match bytes {
                        1 => self.read8(address)? as i8 as i64 as u64,
                        2 => self.read16(address)? as i16 as i64 as u64,
                        4 => self.read32(address)? as i32 as i64 as u64,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                3 if bytes <= 2 => {
                    let value = match bytes {
                        1 => self.read8(address)? as i8 as i32 as u32 as u64,
                        2 => self.read16(address)? as i16 as i32 as u32 as u64,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                _ => return self.unsupported(pc, insn),
            }
            self.pc = next;
            return Ok(());
        }
        // Integer LDR/STR unscaled, pre-indexed and post-indexed forms.
        if insn & 0x3b20_0000 == 0x3800_0000 && (insn >> 10) & 3 != 2 {
            let bytes = 1u64 << (insn >> 30);
            let operation = (insn >> 22) & 3;
            let mode = (insn >> 10) & 3;
            let offset = sign_extend(((insn >> 12) & 0x1ff) as u64, 9);
            let rn = (insn >> 5) & 31;
            let base = self.x_or_sp(rn);
            let address = if mode == 1 {
                base
            } else {
                base.wrapping_add(offset as u64)
            };
            let rt = insn & 31;
            match operation {
                0 => match bytes {
                    1 => self.write8(address, self.x(rt) as u8)?,
                    2 => self.write16(address, self.x(rt) as u16)?,
                    4 => self.write32(address, self.x(rt) as u32)?,
                    8 => self.write64(address, self.x(rt))?,
                    _ => unreachable!(),
                },
                1 => {
                    let value = match bytes {
                        1 => self.read8(address)? as u64,
                        2 => self.read16(address)? as u64,
                        4 => self.read32(address)? as u64,
                        8 => self.read64(address)?,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                2 if bytes < 8 => {
                    let value = match bytes {
                        1 => self.read8(address)? as i8 as i64 as u64,
                        2 => self.read16(address)? as i16 as i64 as u64,
                        4 => self.read32(address)? as i32 as i64 as u64,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                3 if bytes <= 2 => {
                    let value = match bytes {
                        1 => self.read8(address)? as i8 as i32 as u32 as u64,
                        2 => self.read16(address)? as i16 as i32 as u32 as u64,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                _ => return self.unsupported(pc, insn),
            }
            if mode != 0 {
                self.set_x_or_sp(rn, base.wrapping_add(offset as u64));
            }
            self.pc = next;
            return Ok(());
        }
        // Integer LDR/STR register offset with UXTW/LSL/SXTW/SXTX indexing.
        if insn & 0x3b20_0c00 == 0x3820_0800 {
            let size_shift = insn >> 30;
            let bytes = 1u64 << size_shift;
            let operation = (insn >> 22) & 3;
            let source = self.x((insn >> 16) & 31);
            let option = (insn >> 13) & 7;
            let mut offset = match option {
                2 => source as u32 as u64,
                3 => source,
                6 => source as u32 as i32 as i64 as u64,
                7 => source as i64 as u64,
                _ => return self.unsupported(pc, insn),
            };
            if insn & (1 << 12) != 0 {
                offset = offset.wrapping_shl(size_shift);
            }
            let address = self.x_or_sp((insn >> 5) & 31).wrapping_add(offset);
            let rt = insn & 31;
            match operation {
                0 => match bytes {
                    1 => self.write8(address, self.x(rt) as u8)?,
                    2 => self.write16(address, self.x(rt) as u16)?,
                    4 => self.write32(address, self.x(rt) as u32)?,
                    8 => self.write64(address, self.x(rt))?,
                    _ => unreachable!(),
                },
                1 => {
                    let value = match bytes {
                        1 => self.read8(address)? as u64,
                        2 => self.read16(address)? as u64,
                        4 => self.read32(address)? as u64,
                        8 => self.read64(address)?,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                2 if bytes < 8 => {
                    let value = match bytes {
                        1 => self.read8(address)? as i8 as i64 as u64,
                        2 => self.read16(address)? as i16 as i64 as u64,
                        4 => self.read32(address)? as i32 as i64 as u64,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                3 if bytes <= 2 => {
                    let value = match bytes {
                        1 => self.read8(address)? as i8 as i32 as u32 as u64,
                        2 => self.read16(address)? as i16 as i32 as u32 as u64,
                        _ => unreachable!(),
                    };
                    self.set(rt, value);
                }
                _ => return self.unsupported(pc, insn),
            }
            self.pc = next;
            return Ok(());
        }
        // LDP/STP X registers in post-index, signed-offset and pre-index modes.
        if insn & 0x3e00_0000 == 0x2800_0000 && insn >> 30 == 2 && matches!((insn >> 23) & 3, 1..=3)
        {
            let load = insn & 0x0040_0000 != 0;
            let mode = (insn >> 23) & 3;
            let offset = sign_extend(((insn >> 15) & 0x7f) as u64, 7) * 8;
            let rn = (insn >> 5) & 31;
            let base = self.x_or_sp(rn);
            let address = if mode == 1 {
                base
            } else {
                base.wrapping_add(offset as u64)
            };
            let rt = insn & 31;
            let rt2 = (insn >> 10) & 31;
            if load {
                let first = self.read64(address)?;
                let second = self.read64(address + 8)?;
                self.set(rt, first);
                self.set(rt2, second);
            } else {
                self.write64(address, self.x(rt))?;
                self.write64(address + 8, self.x(rt2))?;
            }
            if matches!(mode, 1 | 3) {
                self.set_x_or_sp(rn, base.wrapping_add(offset as u64));
            }
            self.pc = next;
            return Ok(());
        }
        // Architectural hints including NOP, YIELD, WFE and SEV are safe
        // cooperative no-ops in this single-vCPU executor.
        if insn & 0xffff_f01f == 0xd503_201f {
            self.pc = next;
            return Ok(());
        }
        // DSB/DMB/ISB are ordering points in this single-threaded interpreter.
        if matches!(insn & 0xffff_f0ff, 0xd503_309f | 0xd503_30bf | 0xd503_30df) {
            self.pc = next;
            return Ok(());
        }
        // Guest cache maintenance is synchronous on Relay's coherent backing
        // arena. Linux's early IVAC/CIVAC operations therefore need no host
        // cache action, but remain explicit rather than accepting arbitrary
        // SYS instructions.
        if matches!(insn & 0xffff_ffe0, 0xd508_7620 | 0xd50b_7e20) {
            self.pc = next;
            return Ok(());
        }
        // DC ZVA materially clears one cache block; Linux uses it as a fast
        // zeroing primitive for page-table and page initialization.
        if insn & 0xffff_ffe0 == 0xd50b_7420 {
            let address = self.x(insn & 31) & !63;
            self.memory.write(self.physical(address)?, &[0; 64])?;
            self.pc = next;
            return Ok(());
        }
        // Relay performs page-table walks directly and has no cached TLB.
        if matches!(insn, 0xd508_871f | 0xd508_751f) {
            self.pc = next;
            return Ok(());
        }
        self.unsupported(pc, insn)
    }
    fn unsupported<T>(&self, pc: u64, word: u32) -> Result<T, RelayError> {
        Err(RelayError::Failed(format!(
            "StaticCpu EL1 first unimplemented insn pc={pc:#x} word={word:#010x}"
        )))
    }
}
fn sign_extend(value: u64, bits: u32) -> i64 {
    ((value << (64 - bits)) as i64) >> (64 - bits)
}

fn low_mask(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn sign_extend_width(value: u64, bits: u32, width: u32) -> u64 {
    if bits == 64 || value & (1u64 << (bits - 1)) == 0 {
        value
    } else {
        value | (low_mask(width) & !low_mask(bits))
    }
}

fn reverse_bytes_in_halfwords(value: u64, width: u32) -> u64 {
    let mut result = 0;
    for shift in (0..width).step_by(16) {
        result |= ((value >> shift) & 0xff) << (shift + 8);
        result |= ((value >> (shift + 8)) & 0xff) << shift;
    }
    result
}

fn decode_logical_immediate(is_64: bool, n: u32, immr: u32, imms: u32) -> Option<u64> {
    if !is_64 && n != 0 {
        return None;
    }
    let encoded = (n << 6) | ((!imms) & 0x3f);
    let len = 31u32.checked_sub(encoded.leading_zeros())?;
    if len < 1 {
        return None;
    }
    let levels = (1u32 << len) - 1;
    let size = 1u32 << len;
    let set_bits = imms & levels;
    if set_bits == levels {
        return None;
    }
    let rotate = immr & levels;
    let element_mask = if size == 64 {
        u64::MAX
    } else {
        (1u64 << size) - 1
    };
    let ones = if set_bits == 63 {
        u64::MAX
    } else {
        (1u64 << (set_bits + 1)) - 1
    };
    let element = if rotate == 0 {
        ones
    } else {
        ((ones >> rotate) | (ones << (size - rotate))) & element_mask
    };
    let width = if is_64 { 64 } else { 32 };
    let mut result = 0;
    for offset in (0..width).step_by(size as usize) {
        result |= element << offset;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::GuestPageSize;
    #[test]
    fn follows_image_branch_before_reporting_unknown_word() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xfa40_5a4du32.to_le_bytes()).unwrap();
        memory.write(4, &0x1400_0001u32.to_le_bytes()).unwrap();
        memory.write(8, &0xd503_201fu32.to_le_bytes()).unwrap();
        memory.write(12, &0xffff_ffffu32.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 8);
        cpu.step().unwrap();
        assert!(cpu
            .step()
            .unwrap_err()
            .to_string()
            .contains("pc=0xc word=0xffffffff"));
    }

    #[test]
    fn stores_and_loads_ram_and_pl011() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xf900_0400u32.to_le_bytes()).unwrap(); // STR X0, [X0,#8]
        memory.write(4, &0xf940_0402u32.to_le_bytes()).unwrap(); // LDR X2, [X0,#8]
        memory.write(8, &0xb900_0003u32.to_le_bytes()).unwrap(); // STR W3, [X0]
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x100);
        cpu.set_x(1, 0xfeed_face);
        cpu.set_x(3, b'R' as u64);
        // Put the test value in X0 after choosing its base for an explicit
        // RAM store with Rt=X1 (encoding below replaces first instruction).
        cpu.memory.write(0, &0xf900_0001u32.to_le_bytes()).unwrap();
        cpu.step().unwrap();
        cpu.memory.write(4, &0xf940_0002u32.to_le_bytes()).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(2), 0xfeed_face);
        cpu.set_x(0, PL011_BASE);
        cpu.pc = 8;
        cpu.step().unwrap();
        assert_eq!(cpu.bus.console(), b"R");
    }

    #[test]
    fn loads_bytes_with_unsigned_offsets() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x3973_0000u32.to_le_bytes()).unwrap(); // LDRB W0,[X0,#3264]
        memory.write(3264, &[0xa5]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 0xa5);
    }

    #[test]
    fn post_indexed_byte_load_updates_pointer() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x3840_1406u32.to_le_bytes()).unwrap(); // LDRB W6,[X0],#1
        memory.write(0x100, &[0xa5]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x100);
        cpu.step().unwrap();
        assert_eq!(cpu.x(6), 0xa5);
        assert_eq!(cpu.x(0), 0x101);
    }

    #[test]
    fn register_offset_byte_load_sign_extends_index() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x3878_caa0u32.to_le_bytes()).unwrap(); // LDRB W0,[X21,W24,SXTW]
        memory.write(0xff, &[0x5a]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(21, 0x100);
        cpu.set_x(24, u32::MAX as u64);
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 0x5a);
    }

    #[test]
    fn compare_flags_drive_conditional_branch() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xf100_041fu32.to_le_bytes()).unwrap(); // SUBS XZR,X0,#1
        memory.write(4, &0x5400_0040u32.to_le_bytes()).unwrap(); // B.EQ +8
        memory.write(12, &0xffff_ffffu32.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 1);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 12);
        assert!(cpu.step().unwrap_err().to_string().contains("pc=0xc"));
    }

    #[test]
    fn conditional_compare_uses_encoded_or_computed_flags() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xfa4f_1824u32.to_le_bytes()).unwrap(); // CCMP X1,#15,#4,NE
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.nzcv = 4;
        cpu.step().unwrap();
        assert_eq!(cpu.nzcv, 4);
        cpu.pc = 0;
        cpu.nzcv = 0;
        cpu.set_x(1, 15);
        cpu.step().unwrap();
        assert_eq!(cpu.nzcv & 4, 4);
    }

    #[test]
    fn tst_logical_immediate_sets_linux_boot_condition_flags() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xf27e_027fu32.to_le_bytes()).unwrap(); // TST X19,#4
        memory.write(4, &0x9a93_03f3u32.to_le_bytes()).unwrap(); // CSEL X19,XZR,X19,EQ
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(19, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.nzcv & 4, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.x(19), 4);
        cpu.pc = 0;
        cpu.set_x(19, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.nzcv & 4, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.x(19), 0);
    }

    #[test]
    fn ubfx_extracts_linux_cache_geometry_field() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd350_4c63u32.to_le_bytes()).unwrap(); // UBFX X3,X3,#16,#4
        memory.write(4, &0x9ac3_2042u32.to_le_bytes()).unwrap(); // LSL X2,X2,X3
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(3, 0x000a_0000);
        cpu.step().unwrap();
        assert_eq!(cpu.x(3), 0xa);
        cpu.set_x(2, 1);
        cpu.set_x(3, 6);
        cpu.step().unwrap();
        assert_eq!(cpu.x(2), 64);
    }

    #[test]
    fn bic_clears_linux_cache_alignment_mask() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x8a23_0021u32.to_le_bytes()).unwrap(); // BIC X1,X1,X3
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0x123f);
        cpu.set_x(3, 0x3f);
        cpu.step().unwrap();
        assert_eq!(cpu.x(1), 0x1200);
    }

    #[test]
    fn cache_loop_add_compare_and_maintenance_execute() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        for (offset, instruction) in [
            (0, 0xd50b_7e20_u32),  // DC CIVAC,X0
            (4, 0xd508_7620_u32),  // DC IVAC,X0
            (8, 0x8b02_0000_u32),  // ADD X0,X0,X2
            (12, 0xeb01_001f_u32), // CMP X0,X1
            (16, 0xd508_871f_u32), // TLBI VMALLE1
        ] {
            memory.write(offset, &instruction.to_le_bytes()).unwrap();
        }
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x1000);
        cpu.set_x(1, 0x1040);
        cpu.set_x(2, 0x40);
        cpu.step().unwrap();
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 0x1040);
        cpu.step().unwrap();
        assert_eq!(cpu.nzcv & 4, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 20);
    }

    #[test]
    fn cache_zero_clears_a_64_byte_block() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd50b_7428u32.to_le_bytes()).unwrap(); // DC ZVA,X8
        memory.write(0x100, &[0xff; 64]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(8, 0x123);
        cpu.step().unwrap();
        let mut cleared = [1; 64];
        cpu.memory.read(0x100, &mut cleared).unwrap();
        assert_eq!(cleared, [0; 64]);
    }

    #[test]
    fn multiply_alias_executes_in_32_bit_linux_path() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x1b02_7ca2u32.to_le_bytes()).unwrap(); // MUL W2,W5,W2
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(2, 7);
        cpu.set_x(5, 6);
        cpu.step().unwrap();
        assert_eq!(cpu.x(2), 42);
    }

    #[test]
    fn reverse_bytes_executes_in_page_table_path() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xdac0_0c84u32.to_le_bytes()).unwrap(); // REV X4,X4
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(4, 0x0123_4567_89ab_cdef);
        cpu.step().unwrap();
        assert_eq!(cpu.x(4), 0xefcd_ab89_6745_2301);
    }

    #[test]
    fn add_extended_signs_a_32_bit_kernel_index() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x8b35_c275u32.to_le_bytes()).unwrap(); // ADD X21,X19,W21,SXTW
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(19, 100);
        cpu.set_x(21, u32::MAX as u64);
        cpu.step().unwrap();
        assert_eq!(cpu.x(21), 99);
    }

    #[test]
    fn mrs_reads_virtual_counter_frequency() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd53b_e000u32.to_le_bytes()).unwrap(); // MRS X0,CNTFRQ_EL0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 24_000_000);
    }

    #[test]
    fn linux_el1_exception_return_uses_programmed_link() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd518_4000u32.to_le_bytes()).unwrap(); // MSR SPSR_EL1,X0
        memory.write(4, &0xd518_403eu32.to_le_bytes()).unwrap(); // MSR ELR_EL1,X30
        memory.write(8, &0xd69f_03e0u32.to_le_bytes()).unwrap(); // ERET
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x3c5);
        cpu.set_x(30, 0x100);
        cpu.step().unwrap();
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.sysregs.spsr_el1, 0x3c5);
        assert_eq!(cpu.pc, 0x100);
    }

    #[test]
    fn linux_entry_stack_pairs_preserve_sp_and_registers() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        for (offset, instruction) in [
            (0, 0x9100_003f_u32), // MOV SP,X1
            (4, 0xa9bf_07f5),     // STP X21,X1,[SP,#-16]!
            (8, 0xa8c1_07f5),     // LDP X21,X1,[SP],#16
        ] {
            memory.write(offset, &instruction.to_le_bytes()).unwrap();
        }
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0x800);
        cpu.set_x(21, 0xfeed_face_cafe_beef);
        cpu.step().unwrap();
        assert_eq!(cpu.sp, 0x800);
        cpu.step().unwrap();
        assert_eq!(cpu.sp, 0x7f0);
        cpu.set_x(1, 0);
        cpu.set_x(21, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.sp, 0x800);
        assert_eq!(cpu.x(1), 0x800);
        assert_eq!(cpu.x(21), 0xfeed_face_cafe_beef);
    }

    #[test]
    fn linux_entry_bit_branches_and_literal_load_execute() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x3600_00b3_u32.to_le_bytes()).unwrap(); // TBZ X19,#0,+20
        memory.write(20, &0x5800_00a2_u32.to_le_bytes()).unwrap(); // LDR X2,+20
        memory
            .write(40, &0x0123_4567_89ab_cdef_u64.to_le_bytes())
            .unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(19, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 20);
        cpu.step().unwrap();
        assert_eq!(cpu.x(2), 0x0123_4567_89ab_cdef);
    }

    #[test]
    fn pc_relative_addresses_keep_immlo_as_low_bits() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x1000_0803_u32.to_le_bytes()).unwrap(); // ADR X3,+0x100
        memory.write(4, &0xf000_0004_u32.to_le_bytes()).unwrap(); // ADRP X4,+3 pages
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(3), 0x100);
        cpu.step().unwrap();
        assert_eq!(cpu.x(4), 0x3000);
    }

    #[test]
    fn indirect_call_and_return_preserve_link_register() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd63f_0040_u32.to_le_bytes()).unwrap(); // BLR X2
        memory.write(16, &0xd65f_03c0_u32.to_le_bytes()).unwrap(); // RET
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(2, 16);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 16);
        assert_eq!(cpu.x(30), 4);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 4);
    }
}

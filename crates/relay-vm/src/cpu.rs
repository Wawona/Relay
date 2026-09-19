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
    fn word(&self) -> Result<u32, RelayError> {
        let mut bytes = [0; 4];
        self.memory.read(self.physical(self.pc)?, &mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }
    fn physical(&self, address: u64) -> Result<u64, RelayError> {
        if self.sysregs.sctlr_el1 & 1 == 0 {
            Ok(address)
        } else {
            crate::mmu::walk_stage1(
                &self.memory,
                address,
                self.sysregs.ttbr0_el1,
                self.memory.page_size(),
            )
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
                self.set(rt, self.sysregs.read(reg)?);
            } else {
                self.sysregs.write(reg, self.x(rt))?;
            }
            self.pc = next;
            return Ok(());
        }
        // B.cond: the early boot path chiefly needs EQ/NE, but model all
        // integer condition encodings so compare loops remain deterministic.
        if insn & 0xff00_0010 == 0x5400_0000 {
            let imm = sign_extend(((insn >> 5) & 0x7ffff) as u64, 19) << 2;
            let n = self.nzcv & 8 != 0;
            let z = self.nzcv & 4 != 0;
            let c = self.nzcv & 2 != 0;
            let v = self.nzcv & 1 != 0;
            let take = match insn & 15 {
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
            };
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
        // AND/ORR/EOR shifted register. MOV aliases are ORR with XZR.
        if insn & 0x1f20_0000 == 0x0a00_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let amount = (insn >> 10) & 63;
            if !is_64 && amount >= 32 {
                return self.unsupported(pc, insn);
            }
            let left = self.x((insn >> 5) & 31);
            let source = self.x((insn >> 16) & 31);
            let right = match (insn >> 22) & 3 {
                0 => source << amount,
                1 => source >> amount,
                2 => ((source as i64) >> amount) as u64,
                _ => return self.unsupported(pc, insn),
            };
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
        // LDR/STR unsigned immediate, 32- and 64-bit forms.
        let class = insn & 0xffc0_0000;
        if matches!(class, 0xf900_0000 | 0xf940_0000 | 0xb900_0000 | 0xb940_0000) {
            let is_64 = class & 0x4000_0000 != 0;
            let load = class & 0x0040_0000 != 0;
            let offset = ((insn >> 10) & 0xfff) as u64 * if is_64 { 8 } else { 4 };
            let address = self.x_or_sp((insn >> 5) & 31).wrapping_add(offset);
            let rt = insn & 31;
            if load {
                self.set(
                    rt,
                    if is_64 {
                        self.read64(address)?
                    } else {
                        self.read32(address)? as u64
                    },
                );
            } else if is_64 {
                self.write64(address, self.x(rt))?;
            } else {
                self.write32(address, self.x(rt) as u32)?;
            }
            self.pc = next;
            return Ok(());
        }
        // LDR/STR unscaled, pre-indexed and post-indexed, 32/64-bit forms.
        if insn & 0x3b20_0000 == 0x3800_0000 && matches!(insn >> 30, 2 | 3) && (insn >> 10) & 3 != 2
        {
            let is_64 = insn >> 30 == 3;
            let load = insn & 0x0040_0000 != 0;
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
            if load {
                self.set(
                    rt,
                    if is_64 {
                        self.read64(address)?
                    } else {
                        self.read32(address)? as u64
                    },
                );
            } else if is_64 {
                self.write64(address, self.x(rt))?;
            } else {
                self.write32(address, self.x(rt) as u32)?;
            }
            if mode != 0 {
                self.set_x_or_sp(rn, base.wrapping_add(offset as u64));
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
    fn mrs_reads_virtual_counter_frequency() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd53b_e000u32.to_le_bytes()).unwrap(); // MRS X0,CNTFRQ_EL0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 24_000_000);
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

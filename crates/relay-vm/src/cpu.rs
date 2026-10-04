//! Jitless, handler-shaped AArch64 EL1 core for guest boot.
//!
//! The CPU owns one GuestMemory arena and never allocates in `step`.  Decoder
//! misses are terminal and include the guest PC plus original instruction.

use crate::{bus::Bus, guest::GuestMemory, sysregs::SysRegs};
use relay_core::RelayError;
use std::cell::Cell;

#[derive(Clone, Copy)]
struct StoreTrace {
    physical: u64,
    pc: u64,
    value: u64,
    bytes: u8,
}

const EMPTY_STORE_TRACE: StoreTrace = StoreTrace {
    physical: u64::MAX,
    pc: 0,
    value: 0,
    bytes: 0,
};

pub(crate) struct StaticCpu {
    x: [u64; 31],
    v: [u128; 32],
    sp: u64,
    sp_el1: u64,
    nzcv: u8,
    pub(crate) pc: u64,
    last_pc: u64,
    last_insn: u32,
    prior_pc: u64,
    prior_insn: u32,
    memory: GuestMemory,
    bus: Bus,
    exclusive: Option<(u64, u8)>,
    stores: [StoreTrace; 128],
    store_next: usize,
    atomic_stores: [StoreTrace; 128],
    atomic_store_next: usize,
    pending_data_abort: Cell<Option<(u64, bool, u8)>>,
    last_abort_pc: u64,
    last_abort_insn: u32,
    last_abort_prior_pc: u64,
    last_abort_prior_insn: u32,
    last_abort_address: u64,
    last_abort_esr: u64,
    last_abort_x: [u64; 4],
    last_abort_x30: u64,
    last_abort_sp: u64,
    abort_count: u64,
    pub(crate) sysregs: SysRegs,
}

impl StaticCpu {
    pub(crate) fn new(memory: GuestMemory, entry_pc: u64) -> Result<Self, RelayError> {
        Self::with_bus(memory, entry_pc, Bus::default())
    }
    pub(crate) fn new_with_block(
        memory: GuestMemory,
        entry_pc: u64,
        block: crate::virtio_block::BlockDevice,
    ) -> Result<Self, RelayError> {
        Self::with_bus(memory, entry_pc, Bus::with_block(block))
    }
    fn with_bus(memory: GuestMemory, entry_pc: u64, bus: Bus) -> Result<Self, RelayError> {
        Ok(Self {
            x: [0; 31],
            v: [0; 32],
            sp: 0,
            sp_el1: 0,
            nzcv: 0,
            pc: entry_pc,
            last_pc: entry_pc,
            last_insn: 0,
            prior_pc: entry_pc,
            prior_insn: 0,
            memory,
            bus,
            exclusive: None,
            stores: [EMPTY_STORE_TRACE; 128],
            store_next: 0,
            atomic_stores: [EMPTY_STORE_TRACE; 128],
            atomic_store_next: 0,
            pending_data_abort: Cell::new(None),
            last_abort_pc: 0,
            last_abort_insn: 0,
            last_abort_prior_pc: 0,
            last_abort_prior_insn: 0,
            last_abort_address: 0,
            last_abort_esr: 0,
            last_abort_x: [0; 4],
            last_abort_x30: 0,
            last_abort_sp: 0,
            abort_count: 0,
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
            14 | 15 => true,
            _ => false,
        }
    }
    fn word(&self) -> Result<u32, crate::mmu::WalkError> {
        let mut bytes = [0; 4];
        let physical = self.translate(
            self.pc,
            crate::mmu::AccessType::Execute {
                wxn: self.sysregs.sctlr_el1 & (1 << 19) != 0,
            },
            self.sysregs.current_el,
        )?;
        self.memory
            .read(physical, &mut bytes)
            .map_err(|error| crate::mmu::WalkError {
                status: Some(0x10),
                error,
            })?;
        Ok(u32::from_le_bytes(bytes))
    }
    fn physical(&self, address: u64) -> Result<u64, RelayError> {
        self.translate(address, crate::mmu::AccessType::Read, 1)
            .map_err(|fault| fault.error)
    }
    fn translate(
        &self,
        address: u64,
        access: crate::mmu::AccessType,
        current_el: u8,
    ) -> Result<u64, crate::mmu::WalkError> {
        if self.sysregs.sctlr_el1 & 1 == 0 {
            return Ok(address);
        }
        let table = if address >> 63 != 0 {
            self.sysregs.ttbr1_el1
        } else {
            self.sysregs.ttbr0_el1
        };
        crate::mmu::walk_stage1_with_access(
            &self.memory,
            address,
            table,
            self.memory.page_size(),
            access,
            current_el,
            self.translation_address_bits(address),
        )
    }
    fn translation_address_bits(&self, address: u64) -> u32 {
        let shift = if address >> 63 == 0 { 0 } else { 16 };
        64 - ((self.sysregs.tcr_el1 >> shift) & 0x3f) as u32
    }
    fn data_physical(&self, address: u64, write: bool) -> Result<u64, RelayError> {
        let access = if write {
            crate::mmu::AccessType::Write
        } else {
            crate::mmu::AccessType::Read
        };
        self.translate(address, access, self.sysregs.current_el)
            .map_err(|fault| {
                if let Some(status) = fault.status {
                    self.pending_data_abort.set(Some((address, write, status)));
                }
                fault.error
            })
    }
    // Normal-memory accesses may span two virtually adjacent pages whose
    // physical frames and permissions differ. Keep the common single-page
    // path direct; validate both fragments before a split store mutates RAM.
    fn read_ram(&self, address: u64, physical: u64, out: &mut [u8]) -> Result<(), RelayError> {
        let first = page_fragment_len(address, self.memory.page_size().0 as u64, out.len());
        self.memory.read(physical, &mut out[..first])?;
        if first < out.len() {
            let next = self.data_physical(address.wrapping_add(first as u64), false)?;
            self.memory.read(next, &mut out[first..])?;
        }
        Ok(())
    }

    fn write_ram(&mut self, address: u64, physical: u64, input: &[u8]) -> Result<(), RelayError> {
        let first = page_fragment_len(address, self.memory.page_size().0 as u64, input.len());
        if first < input.len() {
            let next = self.data_physical(address.wrapping_add(first as u64), true)?;
            // CPU scalar/SIMD structure accesses are at most 64 bytes. Preflight bounds
            // too, so a failed second frame cannot leave the first modified.
            let mut scratch = [0; 64];
            self.memory.read(physical, &mut scratch[..first])?;
            self.memory.read(next, &mut scratch[first..input.len()])?;
            self.write_ram_fragment(physical, &input[..first])?;
            self.write_ram_fragment(next, &input[first..])?;
        } else {
            self.write_ram_fragment(physical, input)?;
        }
        Ok(())
    }

    fn write_ram_fragment(&mut self, physical: u64, input: &[u8]) -> Result<(), RelayError> {
        self.memory.write(physical, input)?;
        for (index, chunk) in input.chunks(8).enumerate() {
            let mut value = [0; 8];
            value[..chunk.len()].copy_from_slice(chunk);
            self.record_store(
                physical + (index * 8) as u64,
                chunk.len() as u8,
                u64::from_le_bytes(value),
            );
        }
        Ok(())
    }

    fn record_store(&mut self, physical: u64, bytes: u8, value: u64) {
        self.stores[self.store_next] = StoreTrace {
            physical,
            pc: self.pc,
            value,
            bytes,
        };
        self.store_next = (self.store_next + 1) % self.stores.len();
    }
    fn record_atomic_store(&mut self, physical: u64, bytes: u8, value: u64) {
        self.atomic_stores[self.atomic_store_next] = StoreTrace {
            physical,
            pc: self.pc,
            value,
            bytes,
        };
        self.atomic_store_next = (self.atomic_store_next + 1) % self.atomic_stores.len();
    }
    fn read32(&self, address: u64) -> Result<u32, RelayError> {
        let physical = self.data_physical(address, false)?;
        if self.bus.handles(physical) {
            return self.bus.read32(physical);
        }
        let mut bytes = [0; 4];
        self.read_ram(address, physical, &mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }
    fn write32(&mut self, address: u64, value: u32) -> Result<(), RelayError> {
        let physical = self.data_physical(address, true)?;
        if self.bus.handles(physical) {
            self.bus.write32(physical, value)?;
            self.bus.service(&mut self.memory)?;
            return Ok(());
        }
        self.write_ram(address, physical, &value.to_le_bytes())?;
        Ok(())
    }
    fn take_pending_irq(&mut self) -> Result<(), RelayError> {
        if let Some(irq) = self.sysregs.pending_timer_irq() {
            // Architected timers are PPIs delivered through the GIC CPU
            // interface. Keep the level asserted until the guest masks or
            // reprograms the corresponding timer control register.
            self.bus.raise_irq(irq);
        }
        if self.sysregs.daif & 0x80 != 0
            || (self.bus.pending_irq().is_none() && !self.sysregs.timer_pending())
        {
            return Ok(());
        }
        // IRQs share privilege/stack/PSTATE entry with synchronous exceptions.
        // ESR/FAR stay unchanged; the IRQ vector is 0x80 past its sync vector.
        self.enter_exception(self.pc, 0x80);
        Ok(())
    }
    fn enter_data_abort(&mut self, address: u64, write: bool, status: u8) {
        self.enter_abort(address, write, status, false);
    }
    fn enter_instruction_abort(&mut self, address: u64, status: u8) {
        self.enter_abort(address, false, status, true);
    }
    fn enter_abort(&mut self, address: u64, write: bool, status: u8, instruction: bool) {
        self.sysregs.esr_el1 = (if instruction {
            0x8200_0000
        } else {
            0x9200_0000
        }) | (if self.sysregs.current_el == 0 {
            0
        } else {
            0x0400_0000
        }) | u64::from(status)
            | (u64::from(write) << 6);
        self.last_abort_pc = self.pc;
        self.last_abort_insn = if instruction { 0 } else { self.last_insn };
        self.last_abort_prior_pc = self.prior_pc;
        self.last_abort_prior_insn = self.prior_insn;
        self.last_abort_address = address;
        self.last_abort_esr = self.sysregs.esr_el1;
        self.last_abort_x.copy_from_slice(&self.x[..4]);
        self.last_abort_x30 = self.x(30);
        self.last_abort_sp = self.sp;
        self.abort_count = self.abort_count.saturating_add(1);
        self.sysregs.far_el1 = address;
        self.enter_exception(self.pc, 0);
    }
    fn enter_svc(&mut self, immediate: u16, return_pc: u64) {
        // EC=0b010101 and IL=1 for the 32-bit A64 SVC instruction.
        self.sysregs.esr_el1 = 0x5600_0000 | u64::from(immediate);
        self.enter_exception(return_pc, 0);
    }
    fn enter_exception(&mut self, return_pc: u64, vector_offset: u64) {
        self.exclusive = None;
        let from_lower_el = self.sysregs.current_el == 0;
        let using_sp0 = from_lower_el || self.sysregs.spsel == 0;
        let mode = if from_lower_el {
            0
        } else if using_sp0 {
            4
        } else {
            5
        };
        if using_sp0 {
            self.sysregs.sp_el0 = self.sp;
            self.sp = self.sp_el1;
        }
        self.sysregs.elr_el1 = return_pc;
        self.sysregs.spsr_el1 = self.sysregs.daif | (u64::from(self.nzcv) << 28) | mode;
        self.sysregs.daif |= 0x3c0;
        self.sysregs.current_el = 1;
        self.sysregs.spsel = 1;
        self.pc = self
            .sysregs
            .vbar_el1
            .wrapping_add(vector_offset)
            .wrapping_add(if from_lower_el {
                0x400
            } else if using_sp0 {
                0
            } else {
                0x200
            });
    }
    fn read8(&self, address: u64) -> Result<u8, RelayError> {
        let physical = self.data_physical(address, false)?;
        if let Some(value) = self.bus.read8(physical) {
            return Ok(value);
        }
        let mut bytes = [0; 1];
        self.read_ram(address, physical, &mut bytes)?;
        Ok(bytes[0])
    }
    fn write8(&mut self, address: u64, value: u8) -> Result<(), RelayError> {
        let physical = self.data_physical(address, true)?;
        if self.bus.write8(physical, value) {
            return Ok(());
        }
        self.memory.write(physical, &[value])?;
        self.record_store(physical, 1, u64::from(value));
        Ok(())
    }
    fn read16(&self, address: u64) -> Result<u16, RelayError> {
        let physical = self.data_physical(address, false)?;
        if let Some(value) = self.bus.read16(physical) {
            return Ok(value);
        }
        let mut bytes = [0; 2];
        self.read_ram(address, physical, &mut bytes)?;
        Ok(u16::from_le_bytes(bytes))
    }
    fn write16(&mut self, address: u64, value: u16) -> Result<(), RelayError> {
        let physical = self.data_physical(address, true)?;
        if self.bus.write16(physical, value) {
            return Ok(());
        }
        self.write_ram(address, physical, &value.to_le_bytes())?;
        Ok(())
    }
    fn read64(&self, address: u64) -> Result<u64, RelayError> {
        let mut bytes = [0; 8];
        self.read_ram(address, self.data_physical(address, false)?, &mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }
    fn write64(&mut self, address: u64, value: u64) -> Result<(), RelayError> {
        let physical = self.data_physical(address, true)?;
        self.write_ram(address, physical, &value.to_le_bytes())?;
        Ok(())
    }
    fn read_vector(&self, address: u64, bytes: u64) -> Result<u128, RelayError> {
        let mut raw = [0; 16];
        self.read_ram(
            address,
            self.data_physical(address, false)?,
            &mut raw[..bytes as usize],
        )?;
        Ok(u128::from_le_bytes(raw))
    }
    fn write_vector(&mut self, address: u64, bytes: u64, value: u128) -> Result<(), RelayError> {
        let raw = value.to_le_bytes();
        let physical = self.data_physical(address, true)?;
        self.write_ram(address, physical, &raw[..bytes as usize])?;
        Ok(())
    }

    pub(crate) fn run(&mut self, budget: u64) -> Result<(), RelayError> {
        self.run_slice(budget)?;
        Err(RelayError::Failed(format!(
            "StaticCpu instruction budget exhausted at pc={:#x}",
            self.pc
        )))
    }
    pub(crate) fn run_slice(&mut self, budget: u64) -> Result<(), RelayError> {
        for step in 0..budget {
            // Host stream arrivals must progress even while the guest performs
            // no MMIO writes. Bound polling work independently of run budget.
            if step & 4095 == 0 {
                self.bus.service(&mut self.memory)?;
            }
            self.step().map_err(|error| {
                RelayError::Failed(format!(
                    "{error}; executing={:#x}/{:#010x}; x30={:#x}; elr={:#x}; spsr={:#x}",
                    self.last_pc,
                    self.last_insn,
                    self.x(30),
                    self.sysregs.elr_el1,
                    self.sysregs.spsr_el1,
                ))
            })?;
            self.sysregs.tick(1);
            self.take_pending_irq()?;
        }
        Ok(())
    }
    pub(crate) fn listen_vsock(
        &mut self,
        port: u32,
    ) -> Result<std::sync::mpsc::Receiver<crate::vsock::VsockConnection>, RelayError> {
        self.bus.listen_vsock(port)
    }
    pub(crate) fn console(&self) -> &[u8] {
        self.bus.console()
    }
    pub(crate) fn debug_physical_pc(&self) -> u64 {
        self.physical(self.pc).unwrap_or(u64::MAX)
    }
    pub(crate) fn begin_differential_trace(&mut self) {
        self.memory.clear_dirty_pages();
    }
    pub(crate) fn differential_checkpoint(
        &mut self,
        sequence: u64,
        instructions: u64,
        previous_chain: [u8; 32],
    ) -> Result<crate::differential::TraceCheckpoint, RelayError> {
        use crate::differential::{
            finish_checkpoint, hex, hex_u128, hex_u64, DirtyPageHash, TraceCheckpoint,
            TRACE_SCHEMA_VERSION,
        };

        let mut system_registers: Vec<_> = [
            ("sctlr_el1", self.sysregs.sctlr_el1),
            ("cpacr_el1", self.sysregs.cpacr_el1),
            ("ttbr0_el1", self.sysregs.ttbr0_el1),
            ("ttbr1_el1", self.sysregs.ttbr1_el1),
            ("tcr_el1", self.sysregs.tcr_el1),
            ("mair_el1", self.sysregs.mair_el1),
            ("vbar_el1", self.sysregs.vbar_el1),
            ("tpidr_el1", self.sysregs.tpidr_el1),
            ("tpidr_el0", self.sysregs.tpidr_el0),
            ("tpidrro_el0", self.sysregs.tpidrro_el0),
            ("contextidr_el1", self.sysregs.contextidr_el1),
            ("cntkctl_el1", self.sysregs.cntkctl_el1),
            ("spsr_el1", self.sysregs.spsr_el1),
            ("elr_el1", self.sysregs.elr_el1),
            ("esr_el1", self.sysregs.esr_el1),
            ("far_el1", self.sysregs.far_el1),
            ("sp_el0", self.sysregs.sp_el0),
            ("daif", self.sysregs.daif),
            ("fpcr", self.sysregs.fpcr),
            ("fpsr", self.sysregs.fpsr),
            ("current_el", u64::from(self.sysregs.current_el)),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), hex_u64(value)))
        .collect();
        for (name, register) in [
            ("spsel", crate::sysregs::SPSEL),
            ("cntpct_el0", crate::sysregs::CNTPCT_EL0),
            ("cntp_tval_el0", crate::sysregs::CNTP_TVAL_EL0),
            ("cntp_ctl_el0", crate::sysregs::CNTP_CTL_EL0),
            ("cntp_cval_el0", crate::sysregs::CNTP_CVAL_EL0),
            ("cntvct_el0", crate::sysregs::CNTVCT_EL0),
            ("cntv_tval_el0", crate::sysregs::CNTV_TVAL_EL0),
            ("cntv_ctl_el0", crate::sysregs::CNTV_CTL_EL0),
            ("cntv_cval_el0", crate::sysregs::CNTV_CVAL_EL0),
        ] {
            system_registers.push((name.to_owned(), hex_u64(self.sysregs.read(register)?)));
        }
        let dirty_pages = self
            .memory
            .take_dirty_page_hashes()?
            .into_iter()
            .map(|(physical_address, sha256)| DirtyPageHash {
                physical_address: hex_u64(physical_address),
                sha256: hex(&sha256),
            })
            .collect();
        finish_checkpoint(
            TraceCheckpoint {
                schema_version: TRACE_SCHEMA_VERSION,
                sequence,
                instructions,
                pc: hex_u64(self.pc),
                physical_pc: hex_u64(self.debug_physical_pc()),
                x: self.x.into_iter().map(hex_u64).collect(),
                vectors: self.v.into_iter().map(hex_u128).collect(),
                sp: hex_u64(self.sp),
                sp_el1: hex_u64(self.sp_el1),
                nzcv: self.nzcv,
                system_registers,
                dirty_pages,
                state_sha256: String::new(),
                chain_sha256: String::new(),
            },
            previous_chain,
        )
    }
    pub(crate) fn debug_device_activity(
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
        self.bus.debug_activity()
    }
    pub(crate) fn debug_registers(
        &self,
    ) -> (
        u64,
        u64,
        u64,
        u64,
        u8,
        u64,
        u64,
        u8,
        u64,
        u64,
        u8,
        u64,
        u32,
        u64,
        u32,
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
        let physical = self.physical(self.x(3)).ok();
        let mut stores = physical.into_iter().flat_map(|physical| {
            self.atomic_stores
                .iter()
                .rev()
                .filter(move |store| store.physical == physical)
                .copied()
        });
        let store = stores.next();
        let prior_store = stores.next();
        (
            self.x(0),
            self.x(1),
            self.x(3),
            self.x(30),
            self.read8(self.x(3)).unwrap_or(0xff),
            store.map_or(0, |store| store.pc),
            store.map_or(0, |store| store.value),
            store.map_or(0, |store| store.bytes),
            prior_store.map_or(0, |store| store.pc),
            prior_store.map_or(0, |store| store.value),
            prior_store.map_or(0, |store| store.bytes),
            self.last_abort_pc,
            self.last_abort_insn,
            self.last_abort_prior_pc,
            self.last_abort_prior_insn,
            self.last_abort_address,
            self.last_abort_esr,
            self.last_abort_x[0],
            self.last_abort_x[1],
            self.last_abort_x[2],
            self.last_abort_x[3],
            self.last_abort_x30,
            self.last_abort_sp,
            self.abort_count,
        )
    }
    pub(crate) fn step(&mut self) -> Result<(), RelayError> {
        self.pending_data_abort.set(None);
        match self.step_inner() {
            Err(_) if self.pending_data_abort.get().is_some() => {
                let (address, write, status) = self.pending_data_abort.take().unwrap();
                self.enter_data_abort(address, write, status);
                Ok(())
            }
            result => result,
        }
    }
    fn step_inner(&mut self) -> Result<(), RelayError> {
        let pc = self.pc;
        let insn = match self.word() {
            Ok(insn) => insn,
            Err(fault) => {
                if let Some(status) = fault.status {
                    self.enter_instruction_abort(pc, status);
                    return Ok(());
                }
                return Err(fault.error);
            }
        };
        self.prior_pc = self.last_pc;
        self.prior_insn = self.last_insn;
        self.last_pc = pc;
        self.last_insn = insn;
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
            let target = pc.wrapping_add(imm as u64);
            if insn & 0x8000_0000 != 0 {
                self.set(30, next);
            }
            self.pc = target;
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
        // Immediate SPSel writes choose the active EL1 stack bank.
        if matches!(insn, 0xd500_40bf | 0xd500_41bf) {
            let old = self.sysregs.read(crate::sysregs::SPSEL)?;
            let new = u64::from(insn == 0xd500_41bf);
            if self.sysregs.current_el == 1 && old != new {
                if new == 0 {
                    self.sp_el1 = self.sp;
                    self.sp = self.sysregs.sp_el0;
                } else {
                    self.sysregs.sp_el0 = self.sp;
                    self.sp = self.sp_el1;
                }
            }
            self.sysregs.write(crate::sysregs::SPSEL, new)?;
            self.pc = next;
            return Ok(());
        }
        // DAIFSet/DAIFClr immediate forms used by local interrupt masking.
        if insn & 0xffff_f0df == 0xd503_40df {
            let mask = u64::from((insn >> 8) & 15) << 6;
            self.sysregs.daif = if insn & (1 << 5) == 0 {
                self.sysregs.daif | mask
            } else {
                self.sysregs.daif & !mask
            };
            self.pc = next;
            return Ok(());
        }
        // Exception return restores PSTATE's interrupt mask from SPSR_EL1
        // before resuming at ELR_EL1. Leaving DAIF.I set here suppresses every
        // later architected timer/virtio IRQ after the first exception.
        if insn == 0xd69f_03e0 {
            let target_mode = self.sysregs.spsr_el1 & 15;
            let target_el = match target_mode {
                0 => 0,
                4 | 5 => 1,
                _ => {
                    return Err(RelayError::Failed(format!(
                        "StaticCpu unsupported ERET target mode {:#x}",
                        self.sysregs.spsr_el1 & 15
                    )))
                }
            };
            if self.sysregs.current_el == 0 || self.sysregs.read(crate::sysregs::SPSEL)? == 0 {
                self.sysregs.sp_el0 = self.sp;
            } else {
                self.sp_el1 = self.sp;
            }
            self.sysregs.current_el = target_el;
            self.sysregs.write(crate::sysregs::SPSEL, target_mode & 1)?;
            self.sp = if target_mode & 1 == 0 {
                self.sysregs.sp_el0
            } else {
                self.sp_el1
            };
            self.sysregs.daif = self.sysregs.spsr_el1 & 0x3c0;
            self.nzcv = ((self.sysregs.spsr_el1 >> 28) & 15) as u8;
            self.pc = self.sysregs.elr_el1;
            return Ok(());
        }
        // HVC #0 is the DT-selected PSCI conduit. Relay exposes one static
        // vCPU, so advertise PSCI 0.2 and the migration type while reporting
        // every topology-changing function as unsupported.
        if insn == 0xd400_0002 {
            let result = match self.x(0) as u32 {
                0x8400_0000 => 0x0000_0002, // PSCI_VERSION: 0.2
                0x8400_0006 => 2,           // MIGRATE_INFO_TYPE: no trusted OS
                _ => u64::MAX,              // PSCI_RET_NOT_SUPPORTED
            };
            self.set(0, result);
            self.pc = next;
            return Ok(());
        }
        // SVC enters the synchronous exception vector. ELR points after the
        // trapping instruction so Linux can return directly to userspace.
        if insn & 0xffe0_001f == 0xd400_0001 {
            self.enter_svc(((insn >> 5) & 0xffff) as u16, next);
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
            let target = self.x((insn >> 5) & 31);
            if insn & 0xffff_fc1f == 0xd63f_0000 {
                self.set(30, next);
            }
            self.pc = target;
            return Ok(());
        }
        // ADC/ADCS/SBC/SBCS, including NGC/NGCS aliases. Linux syscall
        // dispatch uses NGC to turn a bounds comparison into a mask.
        if insn & 0x1fe0_fc00 == 0x1a00_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let mask = low_mask(width);
            let left = self.x((insn >> 5) & 31) & mask;
            let right = self.x((insn >> 16) & 31) & mask;
            let carry_in = u64::from(self.nzcv & 2 != 0);
            let subtract = insn & (1 << 30) != 0;
            let signed = |value: u64| {
                if is_64 {
                    value as i64 as i128
                } else {
                    value as u32 as i32 as i128
                }
            };
            let signed_min = -(1i128 << (width - 1));
            let signed_max = (1i128 << (width - 1)) - 1;
            let (result, carry, overflow) = if subtract {
                let borrow = 1 - carry_in;
                let result = left.wrapping_sub(right).wrapping_sub(borrow) & mask;
                let subtrahend = u128::from(right) + u128::from(borrow);
                let carry = u128::from(left) >= subtrahend;
                let signed_result = signed(left) - signed(right) - i128::from(borrow);
                let overflow = !(signed_min..=signed_max).contains(&signed_result);
                (result, carry, overflow)
            } else {
                let wide = u128::from(left) + u128::from(right) + u128::from(carry_in);
                let result = wide as u64 & mask;
                let carry = wide > u128::from(mask);
                let signed_result = signed(left) + signed(right) + i128::from(carry_in);
                let overflow = !(signed_min..=signed_max).contains(&signed_result);
                (result, carry, overflow)
            };
            self.set(insn & 31, result);
            if insn & (1 << 29) != 0 {
                self.nzcv = ((result >> (width - 1)) as u8) << 3
                    | ((result == 0) as u8) << 2
                    | (carry as u8) << 1
                    | overflow as u8;
            }
            self.pc = next;
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
            if (insn >> 29) & 3 == 3 {
                self.set(insn & 31, result); // ANDS/TST discards Rd=31.
                self.nzcv = ((result >> (width - 1)) as u8) << 3 | ((result == 0) as u8) << 2;
            } else {
                self.set_x_or_sp(insn & 31, result);
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
            let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
            let left = self.x((insn >> 5) & 31);
            let source = self.x((insn >> 16) & 31) & mask;
            let mut right = match (insn >> 22) & 3 {
                0 => source << amount,
                1 => source >> amount,
                2 if is_64 => ((source as i64) >> amount) as u64,
                2 => ((source as u32 as i32) >> amount) as u32 as u64,
                3 if is_64 => source.rotate_right(amount),
                3 => u64::from((source as u32).rotate_right(amount)),
                _ => unreachable!(),
            };
            if insn & (1 << 21) != 0 {
                right = !right;
            }
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
        // EXTR, including the ROR-immediate alias when both source registers
        // are equal. The concatenation is Rn:Rm and extraction starts in Rm.
        if insn & 0x7f80_0000 == 0x1380_0000 {
            let is_64 = insn & 0x8000_0000 != 0;
            let width = if is_64 { 64 } else { 32 };
            let shift = (insn >> 10) & 63;
            if shift >= width || ((insn >> 22) & 1 != u32::from(is_64)) {
                return self.unsupported(pc, insn);
            }
            let mask = low_mask(width);
            let high = self.x((insn >> 5) & 31) & mask;
            let low = self.x((insn >> 16) & 31) & mask;
            let result = if shift == 0 {
                low
            } else {
                (low >> shift) | ((high << (width - shift)) & mask)
            };
            self.set(insn & 31, result & mask);
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
        // SMADDL/SMSUBL/UMADDL/UMSUBL and SMULL/UMULL aliases.
        if insn & 0x7f20_0000 == 0x1b20_0000 {
            let left = self.x((insn >> 5) & 31) as u32;
            let right = self.x((insn >> 16) & 31) as u32;
            let product = if insn & (1 << 23) != 0 {
                u64::from(left).wrapping_mul(u64::from(right))
            } else {
                (left as i32 as i64).wrapping_mul(right as i32 as i64) as u64
            };
            let addend = self.x((insn >> 10) & 31);
            let result = if insn & (1 << 15) == 0 {
                product.wrapping_add(addend)
            } else {
                addend.wrapping_sub(product)
            };
            self.set(insn & 31, result);
            self.pc = next;
            return Ok(());
        }
        // SMULH/UMULH return the high 64 bits of a full 128-bit product.
        if matches!(insn & 0xffe0_fc00, 0x9b40_7c00 | 0x9bc0_7c00) {
            let left = self.x((insn >> 5) & 31);
            let right = self.x((insn >> 16) & 31);
            let result = if insn & (1 << 23) != 0 {
                ((u128::from(left) * u128::from(right)) >> 64) as u64
            } else {
                (((left as i64 as i128) * (right as i64 as i128)) >> 64) as u64
            };
            self.set(insn & 31, result);
            self.pc = next;
            return Ok(());
        }
        // UDIV/SDIV, 32- and 64-bit forms. Linux uses the 32-bit unsigned
        // form while deriving early cache and page-table geometry. AArch64
        // defines divide-by-zero to produce zero rather than an exception.
        if matches!(insn & 0x7fe0_fc00, 0x1ac0_0800 | 0x1ac0_0c00) {
            let is_64 = insn & 0x8000_0000 != 0;
            let left = self.x((insn >> 5) & 31);
            let right = self.x((insn >> 16) & 31);
            let signed = insn & 0x400 != 0;
            let result = match (is_64, signed) {
                (false, false) => match right as u32 {
                    0 => 0,
                    divisor => u64::from(left as u32 / divisor),
                },
                (false, true) => match right as i32 {
                    0 => 0,
                    divisor => (left as i32).wrapping_div(divisor) as u32 as u64,
                },
                (true, false) => match right {
                    0 => 0,
                    divisor => left / divisor,
                },
                (true, true) => match right as i64 {
                    0 => 0,
                    divisor => (left as i64).wrapping_div(divisor) as u64,
                },
            };
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
        // AdvSIMD modified immediates: MOVI/MVNI, ORR/BIC and FMOV.
        if insn & 0x9ff8_0c00 == 0x0f00_0400 {
            let immediate = ((((insn >> 16) & 7) << 5) | ((insn >> 5) & 31)) as u8;
            let mode = (insn >> 12) & 15;
            let op = insn & (1 << 29) != 0;
            let wide = insn & (1 << 30) != 0;
            if mode == 15 && op && !wide {
                return self.unsupported(pc, insn);
            }
            let (element, bits) = match mode {
                0..=7 => (u64::from(immediate) << ((mode / 2) * 8), 32),
                8..=11 => (u64::from(immediate) << (((mode - 8) / 2) * 8), 16),
                12..=13 => {
                    let shift = (mode - 11) * 8;
                    ((u64::from(immediate) << shift) | low_mask(shift), 32)
                }
                14 if !op => (u64::from(immediate), 8),
                14 => (
                    (0..8).fold(0u64, |value, byte| {
                        value
                            | if immediate & (1 << byte) != 0 {
                                0xff << (byte * 8)
                            } else {
                                0
                            }
                    }),
                    64,
                ),
                _ => (expand_fp_immediate(immediate, op), if op { 64 } else { 32 }),
            };
            let mut expanded = 0u128;
            for lane in 0..(if wide { 128 } else { 64 } / bits) {
                expanded |= u128::from(element) << (lane * bits);
            }
            let rd = (insn & 31) as usize;
            let result = if mode < 12 && mode & 1 != 0 {
                if op {
                    self.v[rd] & !expanded
                } else {
                    self.v[rd] | expanded
                }
            } else if mode < 14 && op {
                !expanded
            } else {
                expanded
            };
            self.v[rd] = if wide {
                result
            } else {
                result & u128::from(u64::MAX)
            };
            self.pc = next;
            return Ok(());
        }
        // FCVTN/FCVTN2 and FCVTL/FCVTL2, double/single precision only.
        // Snapshot aliased sources and reuse the integer-only FPCR/FPSR kernels.
        match insn & 0xbfff_fc00 {
            0x0e61_6800 => {
                let source = self.v[((insn >> 5) & 31) as usize];
                let rd = (insn & 31) as usize;
                let mut result = 0u64;
                for lane in 0..2 {
                    let (value, flags) = crate::float_convert::narrow_double(
                        (source >> (lane * 64)) as u64,
                        self.sysregs.fpcr,
                    );
                    result |= u64::from(value) << (lane * 32);
                    self.sysregs.fpsr |= flags;
                }
                self.v[rd] = if insn & (1 << 30) != 0 {
                    (self.v[rd] & u128::from(u64::MAX)) | (u128::from(result) << 64)
                } else {
                    u128::from(result)
                };
                self.pc = next;
                return Ok(());
            }
            0x0e61_7800 => {
                let source = self.v[((insn >> 5) & 31) as usize];
                let first = if insn & (1 << 30) != 0 { 2 } else { 0 };
                let mut result = 0u128;
                for lane in 0..2 {
                    let (value, flags) = crate::float_convert::widen_single(
                        (source >> ((first + lane) * 32)) as u32,
                        self.sysregs.fpcr,
                    );
                    result |= u128::from(value) << (lane * 64);
                    self.sysregs.fpsr |= flags;
                }
                self.v[(insn & 31) as usize] = result;
                self.pc = next;
                return Ok(());
            }
            _ => {}
        }
        // XTN/XTN2 truncate each source lane. The upper-half form retains
        // the old low half; the lower-half form clears the old high half.
        if insn & 0xbf3f_fc00 == 0x0e21_2800 {
            let size = (insn >> 22) & 3;
            if size == 3 {
                return self.unsupported(pc, insn);
            }
            let bits = 8 << size;
            let source = self.v[((insn >> 5) & 31) as usize];
            let mut narrowed = 0u64;
            for lane in 0..(64 / bits) {
                narrowed |=
                    ((source >> (lane * bits * 2)) as u64 & low_mask(bits)) << (lane * bits);
            }
            let rd = (insn & 31) as usize;
            self.v[rd] = if insn & (1 << 30) != 0 {
                (self.v[rd] & u128::from(u64::MAX)) | (u128::from(narrowed) << 64)
            } else {
                u128::from(narrowed)
            };
            self.pc = next;
            return Ok(());
        }
        // Non-saturating immediate shifts, including rounding, accumulation
        // and insertion. A wider temporary preserves rounding at shift=64.
        let shift_opcode = insn & 0x8f80_fc00;
        if matches!(
            shift_opcode,
            0x0f00_0400 | 0x0f00_1400 | 0x0f00_2400 | 0x0f00_3400 | 0x0f00_4400 | 0x0f00_5400
        ) {
            let immediate = (insn >> 16) & 127;
            let scalar = insn & (1 << 28) != 0;
            let wide = insn & (1 << 30) != 0;
            let unsigned = insn & (1 << 29) != 0;
            if immediate < 8 || (scalar && !wide) || (shift_opcode == 0x0f00_4400 && !unsigned) {
                return self.unsupported(pc, insn);
            }
            let bits = 1 << (31 - immediate.leading_zeros());
            if (scalar && bits != 64) || (!wide && bits == 64) {
                return self.unsupported(pc, insn);
            }
            let left = shift_opcode == 0x0f00_5400;
            let shift = if left {
                immediate - bits
            } else {
                bits * 2 - immediate
            };
            let mask = low_mask(bits);
            let rd = (insn & 31) as usize;
            let source = self.v[((insn >> 5) & 31) as usize];
            let previous = self.v[rd];
            let mut result = 0u128;
            for lane in 0..(if wide && !scalar { 128 } else { 64 } / bits) {
                let value = (source >> (lane * bits)) as u64 & mask;
                let prior = (previous >> (lane * bits)) as u64 & mask;
                let shifted = simd_immediate_shift(
                    value,
                    prior,
                    bits,
                    shift,
                    ((shift_opcode >> 12) & 7) as u8,
                    unsigned,
                );
                result |= u128::from(shifted & mask) << (lane * bits);
            }
            self.v[rd] = result;
            self.pc = next;
            return Ok(());
        }
        // SSHLL/USHLL and their upper-half forms widen before shifting.
        if insn & 0x9f80_fc00 == 0x0f00_a400 {
            let immediate = (insn >> 16) & 127;
            if !(8..64).contains(&immediate) {
                return self.unsupported(pc, insn);
            }
            let bits = 1 << (31 - immediate.leading_zeros());
            let shift = immediate - bits;
            let offset = if insn & (1 << 30) != 0 { 64 } else { 0 };
            let source = self.v[((insn >> 5) & 31) as usize] >> offset;
            let mask = low_mask(bits);
            let result_mask = low_mask(bits * 2);
            let mut result = 0u128;
            for lane in 0..(64 / bits) {
                let value = (source >> (lane * bits)) as u64 & mask;
                let extended = if insn & (1 << 29) != 0 {
                    value
                } else {
                    sign_extend(value, bits) as u64
                };
                result |= u128::from((extended << shift) & result_mask) << (lane * bits * 2);
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // Integer vector ADD/SUB wraps independently within each lane.
        // Scalar integer forms require Q=1; Q=0 overlaps FCCMP HI.
        if matches!(insn & 0xdf20_fc00, 0x0e20_8400 | 0x4e20_8400 | 0x5e20_8400) {
            let size = (insn >> 22) & 3;
            let scalar = insn & (1 << 28) != 0;
            let wide = !scalar && insn & (1 << 30) != 0;
            if (scalar && (size != 3 || insn & (1 << 30) == 0)) || (!scalar && size == 3 && !wide) {
                return self.unsupported(pc, insn);
            }
            let bits = 8 << size;
            let mask = u128::from(low_mask(bits));
            let left = self.v[((insn >> 5) & 31) as usize];
            let right = self.v[((insn >> 16) & 31) as usize];
            let mut result = 0u128;
            for lane in 0..(if wide { 128 } else { 64 } / bits) {
                let a = (left >> (lane * bits)) & mask;
                let b = (right >> (lane * bits)) & mask;
                let value = if insn & (1 << 29) != 0 {
                    a.wrapping_sub(b)
                } else {
                    a + b
                };
                result |= (value & mask) << (lane * bits);
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // INS from a general register or vector element preserves all
        // destination bits outside the selected lane.
        let insert_general = insn & 0xffe0_fc00 == 0x4e00_1c00;
        if insert_general || insn & 0xffe0_8400 == 0x6e00_0400 {
            let immediate = ((insn >> 16) & 31) as u8;
            let size = immediate.trailing_zeros();
            if size > 3 {
                return self.unsupported(pc, insn);
            }
            let bits = 8 << size;
            let source = if insert_general {
                self.x((insn >> 5) & 31)
            } else {
                let lane = ((insn >> 11) & 15) >> size;
                (self.v[((insn >> 5) & 31) as usize] >> (lane * bits)) as u64
            };
            let destination_lane = u32::from(immediate) >> (size + 1);
            let shift = destination_lane * bits;
            let mask = u128::from(low_mask(bits));
            let rd = (insn & 31) as usize;
            self.v[rd] = (self.v[rd] & !(mask << shift)) | ((u128::from(source) & mask) << shift);
            self.pc = next;
            return Ok(());
        }
        // DUP (general), all legal B/H/S/D vector widths. The lowest set
        // immediate bit selects element size; higher immediate bits are ignored.
        if insn & 0xbfe0_fc00 == 0x0e00_0c00 {
            let immediate = ((insn >> 16) & 31) as u8;
            let size_bit = immediate & immediate.wrapping_neg();
            let Some(result) = duplicate_simd_lane(
                u128::from(self.x((insn >> 5) & 31)),
                size_bit,
                insn & (1 << 30) != 0,
            ) else {
                return self.unsupported(pc, insn);
            };
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // Scalar DUP extracts one lane and clears the rest of Vd. Reuse the
        // checked lane extraction kernel used by UMOV, including D lanes.
        if insn & 0xffe0_fc00 == 0x5e00_0400 {
            let immediate = ((insn >> 16) & 31) as u8;
            let Some(value) = simd_lane_to_gpr(
                self.v[((insn >> 5) & 31) as usize],
                immediate,
                immediate.trailing_zeros() == 3,
                false,
            ) else {
                return self.unsupported(pc, insn);
            };
            self.v[(insn & 31) as usize] = u128::from(value);
            self.pc = next;
            return Ok(());
        }
        // DUP (element), all B/H/S/D source lanes and legal vector widths.
        if insn & 0xbfe0_fc00 == 0x0e00_0400 {
            let Some(result) = duplicate_simd_lane(
                self.v[((insn >> 5) & 31) as usize],
                ((insn >> 16) & 31) as u8,
                insn & (1 << 30) != 0,
            ) else {
                return self.unsupported(pc, insn);
            };
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // ZIP interleaves halves, UZP deinterleaves even/odd lanes and TRN
        // interleaves even/odd lanes. Read both sources before writing Rd.
        if matches!(
            insn & 0xbf20_fc00,
            0x0e00_1800 | 0x0e00_5800 | 0x0e00_2800 | 0x0e00_6800 | 0x0e00_3800 | 0x0e00_7800
        ) {
            let size = (insn >> 22) & 3;
            let wide = insn & (1 << 30) != 0;
            if size == 3 && !wide {
                return self.unsupported(pc, insn);
            }
            let bits = 8 << size;
            let lanes = if wide { 128 } else { 64 } / bits;
            let sources = [
                self.v[((insn >> 5) & 31) as usize],
                self.v[((insn >> 16) & 31) as usize],
            ];
            let part = (insn >> 14) & 1;
            let kind = (insn >> 12) & 3;
            let mut result = 0u128;
            for lane in 0..lanes {
                let (source, index) = match kind {
                    1 => (lane / (lanes / 2), (lane % (lanes / 2)) * 2 + part),
                    2 => (lane % 2, (lane / 2) * 2 + part),
                    _ => (lane % 2, lane / 2 + part * (lanes / 2)),
                };
                let value =
                    (sources[source as usize] >> (index * bits)) & u128::from(low_mask(bits));
                result |= value << (lane * bits);
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // EXT selects one vector-width byte window from Rn concatenated
        // with Rm. Only the active source bytes participate in the 64-bit form.
        if insn & 0xbfe0_8400 == 0x2e00_0000 {
            let bytes = if insn & (1 << 30) != 0 { 16 } else { 8 };
            let offset = ((insn >> 11) & 15) as usize;
            if offset >= bytes {
                return self.unsupported(pc, insn);
            }
            let left = self.v[((insn >> 5) & 31) as usize].to_le_bytes();
            let right = self.v[((insn >> 16) & 31) as usize].to_le_bytes();
            let mut combined = [0u8; 32];
            combined[..bytes].copy_from_slice(&left[..bytes]);
            combined[bytes..bytes * 2].copy_from_slice(&right[..bytes]);
            let mut result = [0u8; 16];
            result[..bytes].copy_from_slice(&combined[offset..offset + bytes]);
            self.v[(insn & 31) as usize] = u128::from_le_bytes(result);
            self.pc = next;
            return Ok(());
        }
        // Single-structure LD/ST1..4 lane transfers and LD1R..4R broadcasts.
        // Q selects the lane's upper index bit, not a destructive vector width.
        if insn & 0xbf00_0000 == 0x0d00_0000 {
            let q = (insn >> 30) & 1;
            let s = (insn >> 12) & 1;
            let size = (insn >> 10) & 3;
            let opcode = (insn >> 13) & 7;
            let count = (((opcode & 1) << 1) | ((insn >> 21) & 1)) as usize + 1;
            let load = insn & (1 << 22) != 0;
            let replicate = opcode >> 1 == 3;
            let (bits, lane) = match opcode >> 1 {
                0 => (8, (q << 3) | (s << 2) | size),
                1 if size & 1 == 0 => (16, (q << 2) | (s << 1) | (size >> 1)),
                2 if size == 0 => (32, (q << 1) | s),
                2 if size == 1 && s == 0 => (64, q),
                3 if load && s == 0 => (8 << size, 0),
                _ => return self.unsupported(pc, insn),
            };
            let post = insn & (1 << 23) != 0;
            let rm = (insn >> 16) & 31;
            if !post && rm != 0 {
                return self.unsupported(pc, insn);
            }
            let bytes = (bits / 8) as usize;
            let total = count * bytes;
            let rn = (insn >> 5) & 31;
            let rt = (insn & 31) as usize;
            let address = self.x_or_sp(rn);
            let advance = if rm == 31 { total as u64 } else { self.x(rm) };
            let physical = self.data_physical(address, !load)?;
            let mut raw = [0; 32];
            if load {
                self.read_ram(address, physical, &mut raw[..total])?;
                for index in 0..count {
                    let mut element = [0; 8];
                    element[..bytes].copy_from_slice(&raw[index * bytes..(index + 1) * bytes]);
                    let value = u128::from(u64::from_le_bytes(element));
                    let register = (rt + index) & 31;
                    if replicate {
                        let mut result = 0;
                        for copy in 0..((64 << q) / bits) {
                            result |= value << (copy * bits);
                        }
                        self.v[register] = result;
                    } else {
                        let shift = lane * bits;
                        let mask = u128::from(low_mask(bits)) << shift;
                        self.v[register] = (self.v[register] & !mask) | (value << shift);
                    }
                }
            } else {
                for index in 0..count {
                    let value = (self.v[(rt + index) & 31] >> (lane * bits)) as u64;
                    raw[index * bytes..(index + 1) * bytes]
                        .copy_from_slice(&value.to_le_bytes()[..bytes]);
                }
                self.write_ram(address, physical, &raw[..total])?;
            }
            if post {
                self.set_x_or_sp(rn, address.wrapping_add(advance));
            }
            self.pc = next;
            return Ok(());
        }
        // Multiple-structure LD/ST1..4 share RAM preflight and writeback.
        // LD/ST2..4 interleave element lanes; LD/ST1 is consecutive.
        if insn & 0xbf20_0000 == 0x0c00_0000 {
            let (count, interleaved) = match (insn >> 12) & 15 {
                7 => (1, false),
                10 => (2, false),
                6 => (3, false),
                2 => (4, false),
                8 => (2, true),
                4 => (3, true),
                0 => (4, true),
                _ => return self.unsupported(pc, insn),
            };
            let post = insn & (1 << 23) != 0;
            let rm = (insn >> 16) & 31;
            if !post && rm != 0 {
                return self.unsupported(pc, insn);
            }
            let bytes = if insn & (1 << 30) != 0 { 16 } else { 8 };
            let element = 1 << ((insn >> 10) & 3);
            if interleaved && element == 8 && bytes == 8 {
                return self.unsupported(pc, insn);
            }
            let total = count * bytes;
            let rt = (insn & 31) as usize;
            let rn = (insn >> 5) & 31;
            let address = self.x_or_sp(rn);
            let advance = if rm == 31 { total as u64 } else { self.x(rm) };
            let load = insn & (1 << 22) != 0;
            let physical = self.data_physical(address, !load)?;
            let mut raw = [0; 64];
            if load {
                self.read_ram(address, physical, &mut raw[..total])?;
                for index in 0..count {
                    let mut vector = [0; 16];
                    for byte in 0..bytes {
                        vector[byte] = raw[structure_byte_offset(
                            index,
                            byte,
                            count,
                            bytes,
                            element,
                            interleaved,
                        )];
                    }
                    self.v[(rt + index) & 31] = u128::from_le_bytes(vector);
                }
            } else {
                for index in 0..count {
                    let vector = self.v[(rt + index) & 31].to_le_bytes();
                    for byte in 0..bytes {
                        raw[structure_byte_offset(
                            index,
                            byte,
                            count,
                            bytes,
                            element,
                            interleaved,
                        )] = vector[byte];
                    }
                }
                self.write_ram(address, physical, &raw[..total])?;
            }
            if post {
                self.set_x_or_sp(rn, address.wrapping_add(advance));
            }
            self.pc = next;
            return Ok(());
        }
        // Widening add/subtract shares extension and lane selection across
        // signed/unsigned, long/wide, and lower/upper source forms.
        if insn & 0x9f20_cc00 == 0x0e20_0000 {
            let size = (insn >> 22) & 3;
            if size == 3 {
                return self.unsupported(pc, insn);
            }
            let bits = 8 << size;
            let result_bits = bits * 2;
            let source_mask = low_mask(bits);
            let result_mask = low_mask(result_bits);
            let upper = if insn & (1 << 30) != 0 { 64 } else { 0 };
            let unsigned = insn & (1 << 29) != 0;
            let wide = insn & (1 << 12) != 0;
            let subtract = insn & (1 << 13) != 0;
            let left = self.v[((insn >> 5) & 31) as usize];
            let right = self.v[((insn >> 16) & 31) as usize];
            let extend = |value: u64| {
                let value = value & source_mask;
                if unsigned {
                    value
                } else {
                    sign_extend(value, bits) as u64
                }
            };
            let mut result = 0u128;
            for lane in 0..128 / result_bits {
                let shift = upper + lane * bits;
                let a = if wide {
                    (left >> (lane * result_bits)) as u64 & result_mask
                } else {
                    extend((left >> shift) as u64)
                };
                let b = extend((right >> shift) as u64);
                let value = if subtract {
                    a.wrapping_sub(b)
                } else {
                    a.wrapping_add(b)
                };
                result |= u128::from(value & result_mask) << (lane * result_bits);
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // Vector and scalar-D comparisons share the same lane semantics.
        // Only the Q=1 scalar encoding is in this class; keep scalar FP
        // encodings (Q=0) out of these masks.
        let scalar_compare = insn & 0x5000_0000 == 0x5000_0000;
        let comparison = if scalar_compare {
            insn & !(1 << 28)
        } else {
            insn
        };
        let compare_zero = comparison & 0xbf3f_fc00;
        let compare_register = comparison & 0xbf20_fc00;
        let zero_form = matches!(
            compare_zero,
            0x0e20_9800 | 0x0e20_8800 | 0x2e20_8800 | 0x2e20_9800 | 0x0e20_a800
        );
        if zero_form
            || matches!(
                compare_register,
                0x2e20_8c00 | 0x0e20_8c00 | 0x0e20_3400 | 0x0e20_3c00 | 0x2e20_3400 | 0x2e20_3c00
            )
        {
            let size = (insn >> 22) & 3;
            let wide = insn & (1 << 30) != 0;
            if (size == 3 && !wide) || (scalar_compare && size != 3) {
                return self.unsupported(pc, insn);
            }
            let bits = 8 << size;
            let mask = low_mask(bits);
            let left = self.v[((insn >> 5) & 31) as usize];
            let right = if zero_form {
                0
            } else {
                self.v[((insn >> 16) & 31) as usize]
            };
            let mut result = 0u128;
            for lane in 0..(if wide && !scalar_compare { 128 } else { 64 } / bits) {
                let a = (left >> (lane * bits)) as u64 & mask;
                let b = (right >> (lane * bits)) as u64 & mask;
                let a_signed = sign_extend(a, bits);
                let b_signed = sign_extend(b, bits);
                let matched = if zero_form {
                    match compare_zero {
                        0x0e20_9800 => a == 0,
                        0x0e20_8800 => a_signed > 0,
                        0x2e20_8800 => a_signed >= 0,
                        0x2e20_9800 => a_signed <= 0,
                        _ => a_signed < 0,
                    }
                } else {
                    match compare_register {
                        0x2e20_8c00 => a == b,
                        0x0e20_8c00 => a & b != 0,
                        0x0e20_3400 => a_signed > b_signed,
                        0x0e20_3c00 => a_signed >= b_signed,
                        0x2e20_3400 => a > b,
                        _ => a >= b,
                    }
                };
                if matched {
                    result |= u128::from(mask) << (lane * bits);
                }
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // AdvSIMD bitwise family, including destination-mask forms.
        if insn & 0x9f20_fc00 == 0x0e20_1c00 {
            let rd = (insn & 31) as usize;
            let left = self.v[((insn >> 5) & 31) as usize];
            let right = self.v[((insn >> 16) & 31) as usize];
            let destination = self.v[rd];
            let result = match ((insn >> 27) & 4) | ((insn >> 22) & 3) {
                0 => left & right,
                1 => left & !right,
                2 => left | right,
                3 => left | !right,
                4 => left ^ right,
                5 => (destination & left) | (!destination & right),
                6 => (destination & !right) | (left & right),
                _ => (destination & right) | (left & !right),
            };
            self.v[rd] = if insn & (1 << 30) != 0 {
                result
            } else {
                result & u128::from(u64::MAX)
            };
            self.pc = next;
            return Ok(());
        }
        // Pairwise integer ADD/min/max: lower result half comes from Rn,
        // upper half from Rm; wrapping is local to each element.
        // Scalar ADDP reduces the two D lanes, modulo 64 bits.
        if insn & 0xffff_fc00 == 0x5ef1_b800 {
            let source = self.v[((insn >> 5) & 31) as usize];
            self.v[(insn & 31) as usize] =
                u128::from((source as u64).wrapping_add((source >> 64) as u64));
            self.pc = next;
            return Ok(());
        }
        // S/UADDLP and S/UADALP widen each adjacent input pair. Snapshot
        // both inputs before replacing Rd, including the in-place forms.
        if insn & 0x9f3f_bc00 == 0x0e20_2800 {
            let bits = 8 << ((insn >> 22) & 3);
            if bits == 64 {
                return self.unsupported(pc, insn);
            }
            let wide = insn & (1 << 30) != 0;
            let rd = (insn & 31) as usize;
            let source = self.v[((insn >> 5) & 31) as usize];
            let prior = self.v[rd];
            let output_bits = bits * 2;
            let mut result = 0u128;
            for lane in 0..(if wide { 128 } else { 64 }) / output_bits {
                let value = pairwise_long_lane(
                    (source >> (lane * output_bits)) as u64,
                    (source >> (lane * output_bits + bits)) as u64,
                    (prior >> (lane * output_bits)) as u64,
                    bits,
                    insn & (1 << 29) == 0,
                    insn & (1 << 14) != 0,
                )
                .expect("decoded pairwise input is 8, 16 or 32 bits");
                result |= u128::from(value) << (lane * output_bits);
            }
            self.v[rd] = result; // Q=0 clears the upper 64 bits.
            self.pc = next;
            return Ok(());
        }
        // SQADD/UQADD/SQSUB/UQSUB, scalar and vector. Keep the scalar
        // Q=0 encodings outside this class: they belong to scalar FP.
        if matches!(insn & 0xdf20_dc00, 0x0e20_0c00 | 0x4e20_0c00 | 0x5e20_0c00) {
            let wide = insn & (1 << 30) != 0;
            let scalar = insn & (1 << 28) != 0;
            let bits = 8 << ((insn >> 22) & 3);
            if bits == 64 && !wide {
                return self.unsupported(pc, insn);
            }
            let left = self.v[((insn >> 5) & 31) as usize];
            let right = self.v[((insn >> 16) & 31) as usize];
            let lanes = if scalar {
                1
            } else {
                (if wide { 128 } else { 64 }) / bits
            };
            let mut result = 0u128;
            let mut saturated = false;
            for lane in 0..lanes {
                let (value, clipped) = saturating_lane(
                    (left >> (lane * bits)) as u64,
                    (right >> (lane * bits)) as u64,
                    bits,
                    insn & (1 << 29) == 0,
                    insn & (1 << 13) != 0,
                )
                .expect("decoded saturation width is 8, 16, 32, or 64");
                result |= u128::from(value) << (lane * bits);
                saturated |= clipped;
            }
            self.v[(insn & 31) as usize] = result;
            self.sysregs.fpsr |= u64::from(saturated) << 27;
            self.pc = next;
            return Ok(());
        }
        // ADDP plus pairwise/elementwise integer min/max share lane selection.
        let operation = insn & 0xbf20_fc00;
        if matches!(
            operation,
            0x0e20_bc00
                | 0x0e20_a400
                | 0x0e20_ac00
                | 0x2e20_a400
                | 0x2e20_ac00
                | 0x0e20_6400
                | 0x0e20_6c00
                | 0x2e20_6400
                | 0x2e20_6c00
        ) {
            let size = (insn >> 22) & 3;
            let wide = insn & (1 << 30) != 0;
            if size == 3 && (!wide || operation != 0x0e20_bc00) {
                return self.unsupported(pc, insn);
            }
            let bits = 8 << size;
            let mask = low_mask(bits);
            let lanes = (if wide { 128 } else { 64 }) / bits;
            let sources = [
                self.v[((insn >> 5) & 31) as usize],
                self.v[((insn >> 16) & 31) as usize],
            ];
            let mut result = 0u128;
            for lane in 0..lanes {
                let (a, b) = if insn & (1 << 15) != 0 {
                    let source = sources[(lane / (lanes / 2)) as usize];
                    let index = lane % (lanes / 2) * 2;
                    (
                        (source >> (index * bits)) as u64,
                        (source >> ((index + 1) * bits)) as u64,
                    )
                } else {
                    (
                        (sources[0] >> (lane * bits)) as u64,
                        (sources[1] >> (lane * bits)) as u64,
                    )
                };
                let value = if operation == 0x0e20_bc00 {
                    a.wrapping_add(b) & mask
                } else {
                    integer_minmax_lane(a, b, bits, insn & (1 << 29) == 0, insn & (1 << 11) != 0)
                };
                result |= u128::from(value) << (lane * bits);
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // CLZ/CLS count leading zero/sign bits independently in each lane.
        if insn & 0x9f3f_fc00 == 0x0e20_4800 {
            let size = (insn >> 22) & 3;
            if size == 3 {
                return self.unsupported(pc, insn);
            }
            let bits = 8 << size;
            let lanes = (if insn & (1 << 30) != 0 { 128 } else { 64 }) / bits;
            let source = self.v[((insn >> 5) & 31) as usize];
            let mut result = 0u128;
            for lane in 0..lanes {
                result |= u128::from(leading_count_lane(
                    (source >> (lane * bits)) as u64,
                    bits,
                    insn & (1 << 29) == 0,
                )) << (lane * bits);
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // REV16/REV32/REV64 reverse element order within fixed-width blocks.
        let reverse = insn & 0xbf3f_fc00;
        if matches!(reverse, 0x0e20_0800 | 0x2e20_0800 | 0x0e20_1800) {
            let bits = 8 << ((insn >> 22) & 3);
            let block_bits = match reverse {
                0x0e20_1800 => 16,
                0x2e20_0800 => 32,
                _ => 64,
            };
            if bits >= block_bits {
                return self.unsupported(pc, insn);
            }
            let lanes = (if insn & (1 << 30) != 0 { 128 } else { 64 }) / bits;
            let block_lanes = block_bits / bits;
            let source = self.v[((insn >> 5) & 31) as usize];
            let mut result = 0u128;
            for lane in 0..lanes {
                let index =
                    (lane / block_lanes) * block_lanes + block_lanes - 1 - lane % block_lanes;
                let value = (source >> (index * bits)) & u128::from(low_mask(bits));
                result |= value << (lane * bits);
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // CNT, NOT and RBIT operate independently on each active byte.
        let byte_unary = insn & 0xbfff_fc00;
        if matches!(byte_unary, 0x0e20_5800 | 0x2e20_5800 | 0x2e60_5800) {
            let source = self.v[((insn >> 5) & 31) as usize].to_le_bytes();
            let mut result = [0u8; 16];
            let lanes = if insn & (1 << 30) != 0 { 16 } else { 8 };
            for lane in 0..lanes {
                result[lane] = match byte_unary {
                    0x0e20_5800 => source[lane].count_ones() as u8,
                    0x2e20_5800 => !source[lane],
                    _ => source[lane].reverse_bits(),
                };
            }
            self.v[(insn & 31) as usize] = u128::from_le_bytes(result);
            self.pc = next;
            return Ok(());
        }
        // TBL/TBX concatenate 1..4 registers, wrapping V31 to V0. Capture
        // operands before writing Rd so every permitted overlap is preserved.
        if insn & 0xbfe0_8c00 == 0x0e00_0000 {
            let rn = ((insn >> 5) & 31) as usize;
            let rd = (insn & 31) as usize;
            let tables = std::array::from_fn(|index| self.v[(rn + index) & 31]);
            self.v[rd] = simd_table_lookup(
                tables,
                ((insn >> 13) & 3) as u8 + 1,
                self.v[((insn >> 16) & 31) as usize],
                self.v[rd],
                insn & (1 << 30) != 0,
                insn & (1 << 12) != 0,
            )
            .expect("decoded table register count is 1..4");
            self.pc = next;
            return Ok(());
        }
        // Across-vector integer reductions: add, widening add, signed/unsigned
        // min/max. Every form writes a scalar and clears the other vector bits.
        let reduction = insn & 0xbf3f_fc00;
        let reduce_kind = match reduction {
            0x0e31_b800 => Some(0),
            0x0e30_3800 | 0x2e30_3800 => Some(1),
            0x0e30_a800 | 0x2e30_a800 => Some(2),
            0x0e31_a800 | 0x2e31_a800 => Some(3),
            _ => None,
        };
        if let Some(kind) = reduce_kind {
            let Some(result) = simd_reduce(
                self.v[((insn >> 5) & 31) as usize],
                8 << ((insn >> 22) & 3),
                insn & (1 << 30) != 0,
                insn & (1 << 29) == 0,
                kind,
            ) else {
                return self.unsupported(pc, insn);
            };
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // UMOV/SMOV extract any legal SIMD lane into Wd/Xd. Reject
        // reserved element widths rather than aliasing them to byte zero.
        if matches!(insn & 0xbfe0_fc00, 0x0e00_2c00 | 0x0e00_3c00) {
            let source = self.v[((insn >> 5) & 31) as usize];
            let Some(value) = simd_lane_to_gpr(
                source,
                ((insn >> 16) & 31) as u8,
                insn & (1 << 30) != 0,
                insn & (1 << 12) == 0,
            ) else {
                return self.unsupported(pc, insn);
            };
            self.set(insn & 31, value);
            self.pc = next;
            return Ok(());
        }
        if insn & 0xffff_fc00 == 0x0f0c_8400 {
            let rd = (insn & 31) as usize;
            let rn = ((insn >> 5) & 31) as usize;
            let source = self.v[rn];
            let mut result = 0u128;
            for lane in 0..8 {
                let halfword = ((source >> (lane * 16)) & 0xffff) as u16;
                result |= u128::from((halfword >> 4) as u8) << (lane * 8);
            }
            self.v[rd] = result;
            self.pc = next;
            return Ok(());
        }
        if insn & 0xffe0_fc00 == 0x0e20_4000 {
            let rd = (insn & 31) as usize;
            let rn = ((insn >> 5) & 31) as usize;
            let rm = ((insn >> 16) & 31) as usize;
            let left = self.v[rn];
            let right = self.v[rm];
            let mut result = 0u128;
            for lane in 0..8 {
                let a = ((left >> (lane * 16)) & 0xffff) as u16;
                let b = ((right >> (lane * 16)) & 0xffff) as u16;
                result |= u128::from(a.wrapping_add(b) >> 8) << (lane * 8);
            }
            self.v[rd] = result;
            self.pc = next;
            return Ok(());
        }
        // FMOV to/from D[1] transfers raw bits and preserves the low lane.
        if insn & 0xffff_fc00 == 0x9eaf_0000 {
            let rd = (insn & 31) as usize;
            self.v[rd] =
                u128::from(self.v[rd] as u64) | (u128::from(self.x((insn >> 5) & 31)) << 64);
            self.pc = next;
            return Ok(());
        }
        if insn & 0xffff_fc00 == 0x9eae_0000 {
            self.set(
                insn & 31,
                (self.v[((insn >> 5) & 31) as usize] >> 64) as u64,
            );
            self.pc = next;
            return Ok(());
        }
        if insn & 0xffff_fc00 == 0x9e66_0000 {
            self.set(insn & 31, self.v[((insn >> 5) & 31) as usize] as u64);
            self.pc = next;
            return Ok(());
        }
        if insn & 0xffff_fc00 == 0x9e67_0000 {
            let rd = (insn & 31) as usize;
            self.v[rd] = u128::from(self.x((insn >> 5) & 31));
            self.pc = next;
            return Ok(());
        }
        if insn & 0xffff_fc00 == 0x1e27_0000 {
            let rd = (insn & 31) as usize;
            self.v[rd] = u128::from(self.x((insn >> 5) & 31) as u32);
            self.pc = next;
            return Ok(());
        }
        if insn & 0xffff_fc00 == 0x1e26_0000 {
            self.set(
                insn & 31,
                u64::from(self.v[((insn >> 5) & 31) as usize] as u32),
            );
            self.pc = next;
            return Ok(());
        }
        // Vector FRINT{N,P,M,Z,A,X,I}, 2S/4S/2D. Snapshot an aliased source;
        // inactive lanes must neither raise exceptions nor survive a Q=0 write.
        let vector_round_mode = match insn & 0xbfbf_fc00 {
            0x0e21_8800 => Some(0),
            0x0ea1_8800 => Some(1),
            0x0e21_9800 => Some(2),
            0x0ea1_9800 => Some(3),
            0x2e21_8800 => Some(4),
            0x2e21_9800 => Some(6),
            0x2ea1_9800 => Some(7),
            _ => None,
        };
        if let Some(mode) = vector_round_mode {
            let wide = insn & (1 << 30) != 0;
            let double = insn & (1 << 22) != 0;
            if double && !wide {
                return self.unsupported(pc, insn);
            }
            let source = self.v[((insn >> 5) & 31) as usize];
            let bits = if double { 64 } else { 32 };
            let mut result = 0u128;
            for lane in 0..((if wide { 128 } else { 64 }) / bits) {
                let immediate = if double {
                    8 | (lane << 4)
                } else {
                    4 | (lane << 3)
                };
                let input = simd_lane_to_gpr(source, immediate as u8, double, false)
                    .expect("decoded vector S/D rounding operand is a legal unsigned lane move");
                let (value, flags) =
                    crate::float_arithmetic::round_integral(input, double, mode, self.sysregs.fpcr);
                result |= u128::from(value) << (lane * bits);
                self.sysregs.fpsr |= flags;
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // Scalar FRINT{N,P,M,Z,A,X,I}, S/D. The reserved mode and absent
        // half-precision format must not fall through as another operation.
        if insn & 0xffbc_7c00 == 0x1e24_4000 {
            let mode = (insn >> 15) & 7;
            if mode == 5 {
                return self.unsupported(pc, insn);
            }
            let source = self.v[((insn >> 5) & 31) as usize] as u64;
            let (result, flags) = crate::float_arithmetic::round_integral(
                source,
                insn & (1 << 22) != 0,
                mode,
                self.sysregs.fpcr,
            );
            self.v[(insn & 31) as usize] = u128::from(result);
            self.sysregs.fpsr |= flags;
            self.pc = next;
            return Ok(());
        }
        // FCVT Sd,Dn: integer arithmetic preserves guest rounding and
        // cumulative exceptions without changing the host FP environment.
        if insn & 0xffff_fc00 == 0x1e62_4000 {
            let source = self.v[((insn >> 5) & 31) as usize] as u64;
            let (result, flags) = crate::float_convert::narrow_double(source, self.sysregs.fpcr);
            self.v[(insn & 31) as usize] = u128::from(result);
            self.sysregs.fpsr |= flags;
            self.pc = next;
            return Ok(());
        }
        if insn & 0xffff_fc00 == 0x1e22_c000 {
            let source = self.v[((insn >> 5) & 31) as usize] as u32;
            let (result, flags) = crate::float_convert::widen_single(source, self.sysregs.fpcr);
            self.v[(insn & 31) as usize] = u128::from(result);
            self.sysregs.fpsr |= flags;
            self.pc = next;
            return Ok(());
        }
        // Scalar FMOV immediate, every S/D encoding. Inactive bits clear.
        if insn & 0xffa0_1fe0 == 0x1e20_1000 {
            let value = expand_fp_immediate(((insn >> 13) & 255) as u8, insn & (1 << 22) != 0);
            self.v[(insn & 31) as usize] = u128::from(value);
            self.pc = next;
            return Ok(());
        }
        // Indexed FMUL S/D reuses the lane-move and software-FP kernels.
        // Snapshot both inputs before writing an aliased destination.
        if insn & 0xaf80_f400 == 0x0f80_9000 {
            let scalar = insn & (1 << 28) != 0;
            let wide = insn & (1 << 30) != 0;
            let double = insn & (1 << 22) != 0;
            let low = (insn >> 21) & 1;
            if (scalar || double) && !wide || double && low != 0 {
                return self.unsupported(pc, insn);
            }
            let high = (insn >> 11) & 1;
            let index = if double { high } else { high * 2 + low };
            let immediate = if double {
                8 | (index << 4)
            } else {
                4 | (index << 3)
            };
            let right = simd_lane_to_gpr(
                self.v[((insn >> 16) & 31) as usize],
                immediate as u8,
                double,
                false,
            )
            .expect("decoded indexed S/D operand is a legal unsigned lane move");
            let left = self.v[((insn >> 5) & 31) as usize];
            let bits = if double { 64 } else { 32 };
            let lanes = if scalar {
                1
            } else {
                (if wide { 128 } else { 64 }) / bits
            };
            let mut result = 0u128;
            for lane in 0..lanes {
                let (value, flags) = crate::float_arithmetic::binary(
                    (left >> (lane * bits)) as u64,
                    right,
                    double,
                    crate::float_arithmetic::Operation::Multiply,
                    self.sysregs.fpcr,
                );
                result |= u128::from(value) << (lane * bits);
                self.sysregs.fpsr |= flags;
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // Vector S/D arithmetic shares the scalar software-FP semantics;
        // FPSR accumulates exception flags from every active lane.
        let vector_float = insn & 0xbfa0_fc00;
        if matches!(
            vector_float,
            0x0e20_d400 | 0x0ea0_d400 | 0x2e20_dc00 | 0x2e20_fc00
        ) {
            use crate::float_arithmetic::{binary, Operation};
            let wide = insn & (1 << 30) != 0;
            let double = insn & (1 << 22) != 0;
            if double && !wide {
                return self.unsupported(pc, insn);
            }
            let bits = if double { 64 } else { 32 };
            let left = self.v[((insn >> 5) & 31) as usize];
            let right = self.v[((insn >> 16) & 31) as usize];
            let operation = match vector_float {
                0x0e20_d400 => Operation::Add,
                0x0ea0_d400 => Operation::Subtract,
                0x2e20_dc00 => Operation::Multiply,
                _ => Operation::Divide,
            };
            let mut result = 0u128;
            for lane in 0..(if wide { 128 } else { 64 } / bits) {
                let (value, flags) = binary(
                    (left >> (lane * bits)) as u64,
                    (right >> (lane * bits)) as u64,
                    double,
                    operation,
                    self.sysregs.fpcr,
                );
                result |= u128::from(value) << (lane * bits);
                self.sysregs.fpsr |= flags;
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // Scalar S/D fused multiply-add; FP16/reserved type encodings do not
        // enter this family. Snapshot all sources before an aliased write.
        if insn & 0xff80_0000 == 0x1f00_0000 {
            let negate_addend = insn & (1 << 21) != 0;
            let negate_product = negate_addend ^ (insn & (1 << 15) != 0);
            let (result, flags) = crate::float_arithmetic::fused(
                self.v[((insn >> 5) & 31) as usize] as u64,
                self.v[((insn >> 16) & 31) as usize] as u64,
                self.v[((insn >> 10) & 31) as usize] as u64,
                insn & (1 << 22) != 0,
                negate_product,
                negate_addend,
                self.sysregs.fpcr,
            );
            self.v[(insn & 31) as usize] = u128::from(result);
            self.sysregs.fpsr |= flags;
            self.pc = next;
            return Ok(());
        }
        // Scalar S/D arithmetic uses software FP and the guest rounding mode.
        if matches!(
            insn & 0xffa0_fc00,
            0x1e20_0800 | 0x1e20_1800 | 0x1e20_2800 | 0x1e20_3800
        ) {
            use crate::float_arithmetic::{binary, Operation};
            let operation = match (insn >> 12) & 3 {
                0 => Operation::Multiply,
                1 => Operation::Divide,
                2 => Operation::Add,
                _ => Operation::Subtract,
            };
            let (result, flags) = binary(
                self.v[((insn >> 5) & 31) as usize] as u64,
                self.v[((insn >> 16) & 31) as usize] as u64,
                insn & (1 << 22) != 0,
                operation,
                self.sysregs.fpcr,
            );
            self.v[(insn & 31) as usize] = u128::from(result);
            self.sysregs.fpsr |= flags;
            self.pc = next;
            return Ok(());
        }
        // SIMD-register FCVT{N,A,P,M,Z}{S,U}, scalar and vector S/D.
        let simd_to_integer = insn & 0x8fbf_fc00;
        if matches!(
            simd_to_integer,
            0x0e21_a800 | 0x0e21_c800 | 0x0ea1_a800 | 0x0e21_b800 | 0x0ea1_b800
        ) {
            let scalar = insn & (1 << 28) != 0;
            let wide = insn & (1 << 30) != 0;
            let double = insn & (1 << 22) != 0;
            if !wide && (scalar || double) {
                return self.unsupported(pc, insn);
            }
            let bits = if double { 64 } else { 32 };
            let lanes = if scalar {
                1
            } else {
                (if wide { 128 } else { 64 }) / bits
            };
            let rounding = match simd_to_integer {
                0x0e21_a800 => 0,
                0x0e21_c800 => 2,
                0x0ea1_a800 => 4,
                0x0e21_b800 => 8,
                _ => 12,
            };
            let source = self.v[((insn >> 5) & 31) as usize];
            let mut result = 0u128;
            for lane in 0..lanes {
                let (value, flags) = crate::float_arithmetic::to_integer(
                    (source >> (lane * bits)) as u64,
                    double,
                    double,
                    insn & (1 << 29) == 0,
                    rounding,
                    self.sysregs.fpcr,
                );
                result |= u128::from(value) << (lane * bits);
                self.sysregs.fpsr |= flags;
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // SIMD-register SCVTF/UCVTF, scalar and vector S/D forms.
        if matches!(insn & 0x9fbf_fc00, 0x0e21_d800 | 0x1e21_d800) {
            let scalar = insn & (1 << 28) != 0;
            let wide = insn & (1 << 30) != 0;
            let double = insn & (1 << 22) != 0;
            if !wide && (scalar || double) {
                return self.unsupported(pc, insn);
            }
            let bits = if double { 64 } else { 32 };
            let lanes = if scalar {
                1
            } else {
                (if wide { 128 } else { 64 }) / bits
            };
            let source = self.v[((insn >> 5) & 31) as usize];
            let mut result = 0u128;
            for lane in 0..lanes {
                let (value, flags) = crate::float_convert::integer_to_float(
                    (source >> (lane * bits)) as u64,
                    double,
                    insn & (1 << 29) == 0,
                    double,
                    self.sysregs.fpcr,
                );
                result |= u128::from(value) << (lane * bits);
                self.sysregs.fpsr |= flags;
            }
            self.v[(insn & 31) as usize] = result;
            self.pc = next;
            return Ok(());
        }
        // SCVTF/UCVTF, W/X to S/D. All four guest rounding modes and
        // cumulative inexact flags are computed independently of host FP.
        if insn & 0x7fbe_fc00 == 0x1e22_0000 {
            let (result, flags) = crate::float_convert::integer_to_float(
                self.x((insn >> 5) & 31),
                insn & (1 << 31) != 0,
                insn & (1 << 16) == 0,
                insn & (1 << 22) != 0,
                self.sysregs.fpcr,
            );
            self.v[(insn & 31) as usize] = u128::from(result);
            self.sysregs.fpsr |= flags;
            self.pc = next;
            return Ok(());
        }
        // FMOV/FABS/FNEG are bit operations: even signaling NaNs keep
        // their payload without changing FPSR or being flushed to zero.
        let scalar_unary = insn & 0xffbf_fc00;
        if matches!(scalar_unary, 0x1e20_4000 | 0x1e20_c000 | 0x1e21_4000) {
            let bits = if insn & (1 << 22) != 0 { 64 } else { 32 };
            let mut value = self.v[((insn >> 5) & 31) as usize] as u64 & low_mask(bits);
            let sign = 1u64 << (bits - 1);
            if scalar_unary == 0x1e20_c000 {
                value &= !sign;
            } else if scalar_unary == 0x1e21_4000 {
                value ^= sign;
            }
            self.v[(insn & 31) as usize] = u128::from(value);
            self.pc = next;
            return Ok(());
        }
        // Vector FABS/FNEG, with inactive upper bits cleared for Q=0.
        if insn & 0x9fbf_fc00 == 0x0ea0_f800 {
            let wide = insn & (1 << 30) != 0;
            let double = insn & (1 << 22) != 0;
            if double && !wide {
                return self.unsupported(pc, insn);
            }
            let bits = if double { 64 } else { 32 };
            let mut signs = 0u128;
            for lane in 0..(if wide { 128 } else { 64 } / bits) {
                signs |= 1u128 << ((lane + 1) * bits - 1);
            }
            let source = self.v[((insn >> 5) & 31) as usize];
            let result = if insn & (1 << 29) != 0 {
                source ^ signs
            } else {
                source & !signs
            };
            self.v[(insn & 31) as usize] = if wide {
                result
            } else {
                result & u128::from(u64::MAX)
            };
            self.pc = next;
            return Ok(());
        }
        // FCSEL copies a scalar payload without performing FP arithmetic.
        if insn & 0xffa0_0c00 == 0x1e20_0c00 {
            let source = if self.condition_holds((insn >> 12) & 15) {
                (insn >> 5) & 31
            } else {
                (insn >> 16) & 31
            };
            let bits = if insn & (1 << 22) != 0 { 64 } else { 32 };
            self.v[(insn & 31) as usize] = self.v[source as usize] & u128::from(low_mask(bits));
            self.pc = next;
            return Ok(());
        }
        // FCCMP/FCCMPE S/D. A failed condition installs immediate NZCV
        // without evaluating either operand or changing FP exception flags.
        if insn & 0xffa0_0c00 == 0x1e20_0400 {
            if self.condition_holds((insn >> 12) & 15) {
                let (nzcv, flags) = crate::float_arithmetic::compare(
                    self.v[((insn >> 5) & 31) as usize] as u64,
                    self.v[((insn >> 16) & 31) as usize] as u64,
                    insn & (1 << 22) != 0,
                    insn & 16 != 0,
                    self.sysregs.fpcr,
                );
                self.nzcv = nzcv;
                self.sysregs.fpsr |= flags;
            } else {
                self.nzcv = (insn & 15) as u8;
            }
            self.pc = next;
            return Ok(());
        }
        // FCMP/FCMPE S/D, register or zero. Integer-bit comparison keeps
        // host floating control out of guest NZCV and exception semantics.
        if insn & 0xffa0_fc07 == 0x1e20_2000 {
            let zero = insn & 8 != 0;
            if zero && insn & 0x001f_0000 != 0 {
                return self.unsupported(pc, insn);
            }
            let right = if zero {
                0
            } else {
                self.v[((insn >> 16) & 31) as usize] as u64
            };
            let (nzcv, flags) = crate::float_arithmetic::compare(
                self.v[((insn >> 5) & 31) as usize] as u64,
                right,
                insn & (1 << 22) != 0,
                insn & 16 != 0,
                self.sysregs.fpcr,
            );
            self.nzcv = nzcv;
            self.sysregs.fpsr |= flags;
            self.pc = next;
            return Ok(());
        }
        // FCVT{N,A,P,M,Z}{S,U}, scalar S/D to W/X. Saturation and
        // exception flags follow the instruction's rounding rule.
        if matches!(
            insn & 0x7fbe_fc00,
            0x1e20_0000 | 0x1e24_0000 | 0x1e28_0000 | 0x1e30_0000 | 0x1e38_0000
        ) {
            let (value, flags) = crate::float_arithmetic::to_integer(
                self.v[((insn >> 5) & 31) as usize] as u64,
                insn & (1 << 22) != 0,
                insn & (1 << 31) != 0,
                insn & (1 << 16) == 0,
                (insn >> 17) & 15,
                self.sysregs.fpcr,
            );
            self.set(insn & 31, value);
            self.sysregs.fpsr |= flags;
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
        if insn & 0x3f00_0000 == 0x3900_0000 {
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
                2 if bytes == 8 => {
                    // PRFM is a non-faulting performance hint. StaticCpu has
                    // no host cache predictor, so preserving flow is enough.
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
        // Integer LDR/STR unscaled, unprivileged, pre-indexed and post-indexed
        // forms. StaticCpu runs only the guest kernel, so LDTR/STTR use the
        // same stage-1 access here while retaining their no-writeback mode.
        if insn & 0x3f20_0000 == 0x3800_0000 {
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
            if mode == 2 && self.physical(address).is_err() {
                self.enter_data_abort(address, operation == 0, 0x07);
                return Ok(());
            }
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
                2 if bytes == 8 && mode == 0 => {
                    // PRFUM is a non-faulting cache hint.
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
            if matches!(mode, 1 | 3) {
                self.set_x_or_sp(rn, base.wrapping_add(offset as u64));
            }
            self.pc = next;
            return Ok(());
        }
        // Integer LDR/STR register offset with UXTW/LSL/SXTW/SXTX indexing.
        if insn & 0x3f20_0c00 == 0x3820_0800 {
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
                2 if bytes == 8 => {
                    // Register-offset PRFM is a non-faulting cache hint.
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
        // SIMD/FP single-register loads and stores. V=1 distinguishes these
        // encodings from integer LDR/STR; opc<1> selects a 128-bit Q access.
        if insn & 0x3f00_0000 == 0x3d00_0000 {
            let opc = (insn >> 22) & 3;
            let bytes = if opc & 2 != 0 {
                16
            } else {
                1u64 << (insn >> 30)
            };
            let address = self
                .x_or_sp((insn >> 5) & 31)
                .wrapping_add(u64::from((insn >> 10) & 0xfff) * bytes);
            let rt = (insn & 31) as usize;
            if opc & 1 != 0 {
                self.v[rt] = self.read_vector(address, bytes)?;
            } else {
                self.write_vector(address, bytes, self.v[rt])?;
            }
            self.pc = next;
            return Ok(());
        }
        // SIMD/FP unscaled, post-indexed and pre-indexed single-register
        // forms. Signed nine-bit displacement follows integer addressing.
        if insn & 0x3f20_0000 == 0x3c00_0000 {
            let opc = (insn >> 22) & 3;
            let bytes = if opc & 2 != 0 {
                16
            } else {
                1u64 << (insn >> 30)
            };
            let mode = (insn >> 10) & 3;
            let offset = sign_extend(((insn >> 12) & 0x1ff) as u64, 9);
            let rn = (insn >> 5) & 31;
            let base = self.x_or_sp(rn);
            let address = if mode == 1 {
                base
            } else {
                base.wrapping_add(offset as u64)
            };
            let rt = (insn & 31) as usize;
            if opc & 1 != 0 {
                self.v[rt] = self.read_vector(address, bytes)?;
            } else {
                self.write_vector(address, bytes, self.v[rt])?;
            }
            if matches!(mode, 1 | 3) {
                self.set_x_or_sp(rn, base.wrapping_add(offset as u64));
            }
            self.pc = next;
            return Ok(());
        }
        // SIMD/FP register-offset single-register loads and stores.
        if insn & 0x3f20_0c00 == 0x3c20_0800 {
            let opc = (insn >> 22) & 3;
            let bytes = if opc & 2 != 0 {
                16
            } else {
                1u64 << (insn >> 30)
            };
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
                offset = offset.wrapping_shl(bytes.trailing_zeros());
            }
            let address = self.x_or_sp((insn >> 5) & 31).wrapping_add(offset);
            let rt = (insn & 31) as usize;
            if opc & 1 != 0 {
                self.v[rt] = self.read_vector(address, bytes)?;
            } else {
                self.write_vector(address, bytes, self.v[rt])?;
            }
            self.pc = next;
            return Ok(());
        }
        // SIMD/FP pair loads and stores. Linux user-copy paths use 128-bit
        // Q-register pairs even before userspace starts executing.
        if insn & 0x3e00_0000 == 0x2c00_0000 && matches!((insn >> 23) & 3, 0..=3) {
            let opc = insn >> 30;
            let bytes = match opc {
                0 => 4,
                1 => 8,
                2 => 16,
                _ => return self.unsupported(pc, insn),
            };
            let load = insn & 0x0040_0000 != 0;
            let mode = (insn >> 23) & 3;
            let offset = sign_extend(((insn >> 15) & 0x7f) as u64, 7) * bytes as i64;
            let rn = (insn >> 5) & 31;
            let base = self.x_or_sp(rn);
            let address = if mode == 1 {
                base
            } else {
                base.wrapping_add(offset as u64)
            };
            let rt = (insn & 31) as usize;
            let rt2 = ((insn >> 10) & 31) as usize;
            if load {
                self.v[rt] = self.read_vector(address, bytes)?;
                self.v[rt2] = self.read_vector(address + bytes, bytes)?;
            } else {
                self.write_vector(address, bytes, self.v[rt])?;
                self.write_vector(address + bytes, bytes, self.v[rt2])?;
            }
            if matches!(mode, 1 | 3) {
                self.set_x_or_sp(rn, base.wrapping_add(offset as u64));
            }
            self.pc = next;
            return Ok(());
        }
        // LDP/STP W/X, LDPSW, and non-temporal LDNP/STNP registers. Mode zero
        // is the non-temporal signed-offset form used by Linux clear/copy
        // paths; it never writes the base register.
        if insn & 0x3e00_0000 == 0x2800_0000
            && matches!(insn >> 30, 0..=2)
            && matches!((insn >> 23) & 3, 0..=3)
        {
            let opc = insn >> 30;
            let bytes = if opc == 2 { 8 } else { 4 };
            let load = insn & 0x0040_0000 != 0;
            let mode = (insn >> 23) & 3;
            if opc == 1 && (!load || mode != 2) {
                return self.unsupported(pc, insn);
            }
            let offset = sign_extend(((insn >> 15) & 0x7f) as u64, 7) * i64::from(bytes);
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
                let first = if opc == 2 {
                    self.read64(address)?
                } else if opc == 1 {
                    self.read32(address)? as i32 as i64 as u64
                } else {
                    self.read32(address)? as u64
                };
                let second = if opc == 2 {
                    self.read64(address + 8)?
                } else if opc == 1 {
                    self.read32(address + 4)? as i32 as i64 as u64
                } else {
                    self.read32(address + 4)? as u64
                };
                self.set(rt, first);
                self.set(rt2, second);
            } else if bytes == 8 {
                self.write64(address, self.x(rt))?;
                self.write64(address + 8, self.x(rt2))?;
            } else {
                self.write32(address, self.x(rt) as u32)?;
                self.write32(address + 4, self.x(rt2) as u32)?;
            }
            if matches!(mode, 1 | 3) {
                self.set_x_or_sp(rn, base.wrapping_add(offset as u64));
            }
            self.pc = next;
            return Ok(());
        }
        // LDAR/LDARB/LDARH load-acquire forms. These share much of their
        // encoding with load-exclusive, but they must not establish an
        // exclusive reservation.
        if insn & 0x3fff_fc00 == 0x08df_fc00 {
            let bytes = 1usize << ((insn >> 30) & 3);
            let address = self.x_or_sp((insn >> 5) & 31);
            let value = match bytes {
                1 => u64::from(self.read8(address)?),
                2 => u64::from(self.read16(address)?),
                4 => u64::from(self.read32(address)?),
                8 => self.read64(address)?,
                _ => unreachable!("AArch64 acquire width is 1, 2, 4, or 8"),
            };
            self.set(insn & 31, value);
            self.pc = next;
            return Ok(());
        }
        // STLR/STLRB/STLRH store-release forms. Rs is architecturally fixed
        // to 31, so treating this as STXR waits for a reservation that does
        // not exist and silently drops Linux spinlock unlocks.
        if insn & 0x3fff_fc00 == 0x089f_fc00 {
            let bytes = 1usize << ((insn >> 30) & 3);
            let address = self.x_or_sp((insn >> 5) & 31);
            let physical = self.data_physical(address, true)?;
            let value = self.x(insn & 31);
            match bytes {
                1 => self.write8(address, value as u8)?,
                2 => self.write16(address, value as u16)?,
                4 => self.write32(address, value as u32)?,
                8 => self.write64(address, value)?,
                _ => unreachable!("AArch64 release width is 1, 2, 4, or 8"),
            }
            self.record_atomic_store(physical, bytes as u8, value);
            self.pc = next;
            return Ok(());
        }
        // LDXP/LDAXP and STXP/STLXP pair-exclusive forms. Linux uses these
        // for 128-bit allocator state updates even on a single vCPU.
        if insn & 0x3fff_8000 == 0x087f_0000 {
            let bytes = if insn & 0x4000_0000 != 0 { 8 } else { 4 };
            let address = self.x_or_sp((insn >> 5) & 31);
            let physical = self.data_physical(address, false)?;
            let first = if bytes == 8 {
                self.read64(address)?
            } else {
                u64::from(self.read32(address)?)
            };
            let second = if bytes == 8 {
                self.read64(address + bytes)?
            } else {
                u64::from(self.read32(address + bytes)?)
            };
            self.set(insn & 31, first);
            self.set((insn >> 10) & 31, second);
            self.exclusive = Some((physical, (bytes * 2) as u8));
            self.pc = next;
            return Ok(());
        }
        if insn & 0x3fe0_0000 == 0x0820_0000 {
            let bytes = if insn & 0x4000_0000 != 0 { 8 } else { 4 };
            let address = self.x_or_sp((insn >> 5) & 31);
            let physical = self.data_physical(address, true)?;
            let succeeds = self.exclusive.take() == Some((physical, (bytes * 2) as u8));
            if succeeds {
                let first = self.x(insn & 31);
                let second = self.x((insn >> 10) & 31);
                if bytes == 8 {
                    self.write64(address, first)?;
                    self.write64(address + bytes, second)?;
                } else {
                    self.write32(address, first as u32)?;
                    self.write32(address + bytes, second as u32)?;
                }
                self.record_atomic_store(physical, (bytes * 2) as u8, first);
            }
            self.set((insn >> 16) & 31, u64::from(!succeeds));
            self.pc = next;
            return Ok(());
        }
        // LDXR/LDAXR and STXR/STLXR. A single-vCPU Relay session has no
        // competing observer, but still tracks the exclusive address and size
        // so malformed or mismatched store-exclusive operations fail.
        if insn & 0x3f5f_7c00 == 0x085f_7c00 {
            let bytes = 1usize << ((insn >> 30) & 3);
            let address = self.x_or_sp((insn >> 5) & 31);
            let physical = self.data_physical(address, false)?;
            let value = match bytes {
                1 => u64::from(self.read8(address)?),
                2 => u64::from(self.read16(address)?),
                4 => u64::from(self.read32(address)?),
                8 => self.read64(address)?,
                _ => unreachable!("AArch64 exclusive width is 1, 2, 4, or 8"),
            };
            self.set(insn & 31, value);
            self.exclusive = Some((physical, bytes as u8));
            self.pc = next;
            return Ok(());
        }
        if insn & 0x3f20_7c00 == 0x0800_7c00 {
            let bytes = 1usize << ((insn >> 30) & 3);
            let address = self.x_or_sp((insn >> 5) & 31);
            let physical = self.data_physical(address, true)?;
            let succeeds = self.exclusive.take() == Some((physical, bytes as u8));
            if succeeds {
                let value = self.x(insn & 31);
                match bytes {
                    1 => self.write8(address, value as u8)?,
                    2 => self.write16(address, value as u16)?,
                    4 => self.write32(address, value as u32)?,
                    8 => self.write64(address, value)?,
                    _ => unreachable!("AArch64 exclusive width is 1, 2, 4, or 8"),
                }
                self.record_atomic_store(physical, bytes as u8, value);
            }
            self.set((insn >> 16) & 31, u64::from(!succeeds));
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
        if matches!(
            insn & 0xffff_ffe0,
            0xd508_7620 | 0xd50b_7520 | 0xd50b_7a20 | 0xd50b_7b20 | 0xd50b_7e20
        ) || matches!(insn, 0xd508_711f | 0xd508_751f)
        {
            self.pc = next;
            return Ok(());
        }
        // DC ZVA materially clears one cache block; Linux uses it as a fast
        // zeroing primitive for page-table and page initialization.
        if insn & 0xffff_ffe0 == 0xd50b_7420 {
            let address = self.x(insn & 31) & !63;
            self.memory
                .write(self.data_physical(address, true)?, &[0; 64])?;
            self.pc = next;
            return Ok(());
        }
        // Relay performs page-table walks directly and has no cached TLB.
        if insn & 0xfff0_f000 == 0xd500_8000 {
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
fn expand_fp_immediate(immediate: u8, double: bool) -> u64 {
    let (fraction_bits, sign_bit, base_exponent) = if double {
        (52, 63, 1024u64)
    } else {
        (23, 31, 128)
    };
    let exponent = (if immediate & 64 != 0 {
        base_exponent - 4
    } else {
        base_exponent
    }) + u64::from((immediate >> 4) & 3);
    (u64::from(immediate >> 7) << sign_bit)
        | (exponent << fraction_bits)
        | (u64::from(immediate & 15) << (fraction_bits - 4))
}

fn duplicate_simd_lane(source: u128, imm5: u8, wide: bool) -> Option<u128> {
    if imm5 == 0 || imm5 > 31 {
        return None;
    }
    let size = imm5.trailing_zeros();
    if size > 3 || (size == 3 && !wide) {
        return None;
    }
    let bits = 8 << size;
    let lane = u32::from(imm5) >> (size + 1);
    let value = (source >> (lane * bits)) & u128::from(low_mask(bits));
    let lanes = if wide { 128 } else { 64 } / bits;
    let mut result = 0u128;
    for index in 0..lanes {
        result |= value << (index * bits);
    }
    Some(result)
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(17)]
fn duplicate_lane_fits_selected_vector_width() {
    let source: u128 = kani::any();
    let immediate: u8 = kani::any();
    let wide: bool = kani::any();
    if let Some(result) = duplicate_simd_lane(source, immediate, wide) {
        if !wide {
            assert!(result >> 64 == 0);
        } else {
            assert!(result as u64 == (result >> 64) as u64);
        }
    }
}

fn simd_lane_to_gpr(source: u128, imm5: u8, wide: bool, signed: bool) -> Option<u64> {
    if imm5 == 0 || imm5 > 31 {
        return None;
    }
    let size = imm5.trailing_zeros();
    let legal = if signed {
        size < if wide { 3 } else { 2 }
    } else if wide {
        size == 3
    } else {
        size < 3
    };
    if !legal {
        return None;
    }
    let bits = 8u32 << size;
    let lane = u32::from(imm5) >> (size + 1);
    let value = (source >> (lane * bits)) as u64 & low_mask(bits);
    let value = if signed {
        sign_extend(value, bits) as u64
    } else {
        value
    };
    Some(if wide { value } else { u64::from(value as u32) })
}

#[cfg(kani)]
#[kani::proof]
fn simd_lane_move_width_and_bounds() {
    let source: u128 = kani::any();
    let imm5: u8 = kani::any();
    let wide: bool = kani::any();
    let signed: bool = kani::any();
    if let Some(value) = simd_lane_to_gpr(source, imm5, wide, signed) {
        assert!(imm5 > 0 && imm5 <= 31);
        if !wide {
            assert!(value <= u64::from(u32::MAX));
        }
        let bits = 8u32 << imm5.trailing_zeros();
        let lane = u32::from(imm5) >> (imm5.trailing_zeros() + 1);
        assert!(lane * bits + bits <= 128);
        if !signed {
            assert!(value == ((source >> (lane * bits)) as u64 & low_mask(bits)));
        }
    }
}

// The decoder validates width/shift/operation before calling this pure lane kernel.
fn simd_immediate_shift(
    value: u64,
    prior: u64,
    bits: u32,
    shift: u32,
    operation: u8,
    unsigned: bool,
) -> u64 {
    let mask = low_mask(bits);
    let value = value & mask;
    let prior = prior & mask;
    let result = if operation == 5 {
        (value << shift) | if unsigned { prior & low_mask(shift) } else { 0 }
    } else if operation == 4 {
        ((u128::from(value) >> shift) as u64) | (prior & !low_mask(bits - shift))
    } else {
        let extended = if unsigned {
            i128::from(value)
        } else {
            i128::from(sign_extend(value, bits))
        };
        let rounding = if operation & 2 != 0 {
            1i128 << (shift - 1)
        } else {
            0
        };
        let shifted = ((extended + rounding) >> shift) as u64;
        if operation & 1 != 0 {
            shifted.wrapping_add(prior)
        } else {
            shifted
        }
    };
    result & mask
}

#[cfg(kani)]
#[kani::proof]
fn immediate_shift_lane_bounds_and_full_width_edges() {
    let value: u64 = kani::any();
    let prior: u64 = kani::any();
    let size: u8 = kani::any();
    kani::assume(size <= 3);
    let bits = 8u32 << size;
    let shift: u32 = kani::any();
    let operation: u8 = kani::any();
    let unsigned: bool = kani::any();
    kani::assume(operation <= 5);
    kani::assume(if operation == 5 {
        shift < bits
    } else {
        shift > 0 && shift <= bits
    });
    let result = simd_immediate_shift(value, prior, bits, shift, operation, unsigned);
    assert_eq!(result & !low_mask(bits), 0);
    if operation == 5 && shift == 0 {
        assert_eq!(result, value & low_mask(bits));
    }
    if operation == 4 && shift == bits {
        assert_eq!(result, prior & low_mask(bits));
    }
    if operation == 2 && shift == bits {
        assert_eq!(
            result,
            if unsigned {
                (value >> (bits - 1)) & 1
            } else {
                0
            }
        );
    }
}

fn sign_extend(value: u64, bits: u32) -> i64 {
    ((value << (64 - bits)) as i64) >> (64 - bits)
}

// Caller supplies register<count<=4, byte<bytes in {8,16}, and an element
// size in {1,2,4,8} dividing bytes. The mapping is a bounded permutation.
fn structure_byte_offset(
    register: usize,
    byte: usize,
    count: usize,
    bytes: usize,
    element: usize,
    interleaved: bool,
) -> usize {
    if interleaved {
        (byte / element * count + register) * element + byte % element
    } else {
        register * bytes + byte
    }
}

#[cfg(kani)]
#[kani::proof]
fn structure_byte_layout_is_bounded_and_invertible() {
    let count = usize::from(kani::any::<u8>());
    kani::assume(count >= 1 && count <= 4);
    let bytes = if kani::any::<bool>() { 16 } else { 8 };
    let size: u8 = kani::any();
    kani::assume(size < 4);
    let element = 1usize << size;
    let register = usize::from(kani::any::<u8>());
    let byte = usize::from(kani::any::<u8>());
    kani::assume(register < count && byte < bytes);
    let interleaved: bool = kani::any();
    let offset = structure_byte_offset(register, byte, count, bytes, element, interleaved);
    assert!(offset < count * bytes && offset < 64);
    if interleaved {
        assert_eq!((offset / element) % count, register);
        assert_eq!(
            (offset / (element * count)) * element + offset % element,
            byte
        );
    } else {
        assert_eq!(offset / bytes, register);
        assert_eq!(offset % bytes, byte);
    }
}

// Caller supplies an architectural 8/16/32-bit element width.
fn leading_count_lane(value: u64, bits: u32, signed: bool) -> u32 {
    let value = value & low_mask(bits);
    let normalized = if signed && value & (1 << (bits - 1)) != 0 {
        !value & low_mask(bits)
    } else {
        value
    };
    normalized.leading_zeros() - (64 - bits) - u32::from(signed)
}

#[cfg(kani)]
#[kani::proof]
fn leading_count_matches_prefix() {
    let size: u32 = kani::any();
    kani::assume(size < 3);
    let bits = 8 << size;
    let value: u64 = kani::any();
    let signed: bool = kani::any();
    let count = leading_count_lane(value, bits, signed);
    let skip = u32::from(signed);
    assert!(count <= bits - skip);
    let target = signed && value & (1 << (bits - 1)) != 0;
    let index: u32 = kani::any();
    kani::assume(index < bits - skip);
    let bit = value & (1 << (bits - 1 - skip - index)) != 0;
    if index < count {
        assert_eq!(bit, target);
    } else if index == count {
        assert_ne!(bit, target);
    }
}

// Caller supplies the architectural 8/16/32-bit min/max element width.
fn integer_minmax_lane(left: u64, right: u64, bits: u32, signed: bool, minimum: bool) -> u64 {
    let a = left & low_mask(bits);
    let b = right & low_mask(bits);
    let less = if signed {
        sign_extend(a, bits) < sign_extend(b, bits)
    } else {
        a < b
    };
    if less == minimum {
        a
    } else {
        b
    }
}

#[cfg(kani)]
#[kani::proof]
fn integer_minmax_selects_ordered_input() {
    let size: u32 = kani::any();
    kani::assume(size < 3);
    let bits = 8 << size;
    let left: u64 = kani::any();
    let right: u64 = kani::any();
    let signed: bool = kani::any();
    let minimum: bool = kani::any();
    let mask = (1u64 << bits) - 1;
    let a = left & mask;
    let b = right & mask;
    let selected = integer_minmax_lane(left, right, bits, signed, minimum);
    let value = |x: u64| -> i128 {
        i128::from(x)
            - if signed && x & (1 << (bits - 1)) != 0 {
                1i128 << bits
            } else {
                0
            }
    };
    assert!(selected == a || selected == b);
    if minimum {
        assert!(value(selected) <= value(a) && value(selected) <= value(b));
    } else {
        assert!(value(selected) >= value(a) && value(selected) >= value(b));
    }
}

fn saturating_lane(
    left: u64,
    right: u64,
    bits: u32,
    signed: bool,
    subtract: bool,
) -> Option<(u64, bool)> {
    if !matches!(bits, 8 | 16 | 32 | 64) {
        return None;
    }
    let mask = low_mask(bits);
    let extend = |value: u64| -> i128 {
        if signed {
            i128::from(sign_extend(value & mask, bits))
        } else {
            i128::from(value & mask)
        }
    };
    let left = extend(left);
    let right = extend(right);
    let exact = if subtract { left - right } else { left + right };
    let minimum = if signed { -(1i128 << (bits - 1)) } else { 0 };
    let maximum = (1i128 << (bits - u32::from(signed))) - 1;
    let result = exact.clamp(minimum, maximum);
    Some((result as u64 & mask, result != exact))
}

#[cfg(kani)]
#[kani::proof]
fn saturating_arithmetic_matches_integer_bounds() {
    let left: u64 = kani::any();
    let right: u64 = kani::any();
    let size: u32 = kani::any();
    kani::assume(size < 4);
    let bits = 8 << size;
    let signed: bool = kani::any();
    let subtract: bool = kani::any();
    let (result, saturated) = saturating_lane(left, right, bits, signed, subtract).unwrap();
    let mask = (1u128 << bits) - 1;
    let interpret = |value: u64| -> i128 {
        let raw = u128::from(value) & mask;
        if signed && raw & (1u128 << (bits - 1)) != 0 {
            raw as i128 - (1i128 << bits)
        } else {
            raw as i128
        }
    };
    let exact = if subtract {
        interpret(left) - interpret(right)
    } else {
        interpret(left) + interpret(right)
    };
    let low = if signed { -(1i128 << (bits - 1)) } else { 0 };
    let high = if signed {
        (1i128 << (bits - 1)) - 1
    } else {
        mask as i128
    };
    assert_eq!(saturated, exact < low || exact > high);
    assert_eq!(
        interpret(result),
        if exact < low {
            low
        } else if exact > high {
            high
        } else {
            exact
        }
    );
    assert!(u128::from(result) <= mask);
}

fn simd_table_lookup(
    tables: [u128; 4],
    count: u8,
    indices: u128,
    prior: u128,
    wide: bool,
    extension: bool,
) -> Option<u128> {
    if !(1..=4).contains(&count) {
        return None;
    }
    let mut result = 0u128;
    for lane in 0..if wide { 16 } else { 8 } {
        let index = ((indices >> (lane * 8)) & 255) as usize;
        let byte = if index < usize::from(count) * 16 {
            tables[index / 16] >> ((index % 16) * 8)
        } else if extension {
            prior >> (lane * 8)
        } else {
            0
        } & 255;
        result |= byte << (lane * 8);
    }
    Some(result)
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(17)]
fn table_lookup_matches_selected_byte() {
    let tables: [u128; 4] = kani::any();
    let count: u8 = kani::any();
    kani::assume(count >= 1 && count <= 4);
    let indices: u128 = kani::any();
    let prior: u128 = kani::any();
    let wide: bool = kani::any();
    let extension: bool = kani::any();
    let result = simd_table_lookup(tables, count, indices, prior, wide, extension).unwrap();
    let lane: u32 = kani::any();
    kani::assume(lane < if wide { 16 } else { 8 });
    let index = ((indices >> (lane * 8)) & 255) as usize;
    let expected = if index < usize::from(count) * 16 {
        (tables[index / 16] >> ((index % 16) * 8)) & 255
    } else if extension {
        (prior >> (lane * 8)) & 255
    } else {
        0
    };
    assert_eq!((result >> (lane * 8)) & 255, expected);
    if !wide {
        assert_eq!(result >> 64, 0);
    }
}

/// Adjacent source lanes add exactly in twice their width; optional destination
/// accumulation wraps modulo that widened width. Other lanes/flags are external.
fn pairwise_long_lane(
    a: u64,
    b: u64,
    prior: u64,
    bits: u32,
    signed: bool,
    accumulate: bool,
) -> Option<u64> {
    if !matches!(bits, 8 | 16 | 32) {
        return None;
    }
    let mask = low_mask(bits);
    let a = a & mask;
    let b = b & mask;
    let sum = if signed {
        (sign_extend(a, bits) + sign_extend(b, bits)) as u64
    } else {
        a + b
    };
    Some(sum.wrapping_add(if accumulate { prior } else { 0 }) & low_mask(bits * 2))
}

#[cfg(kani)]
#[kani::proof]
fn pairwise_long_add_matches_widened_modular_sum() {
    let a: u64 = kani::any();
    let b: u64 = kani::any();
    let prior: u64 = kani::any();
    let bits: u32 = kani::any();
    kani::assume(matches!(bits, 8 | 16 | 32));
    let signed: bool = kani::any();
    let accumulate: bool = kani::any();
    let integer = |raw: u64| -> i128 {
        match (bits, signed) {
            (8, true) => raw as i8 as i128,
            (16, true) => raw as i16 as i128,
            (32, true) => raw as i32 as i128,
            (8, false) => raw as u8 as i128,
            (16, false) => raw as u16 as i128,
            _ => raw as u32 as i128,
        }
    };
    let result = pairwise_long_lane(a, b, prior, bits, signed, accumulate).unwrap();
    let modulus = 1i128 << (bits * 2);
    let expected =
        (integer(a) + integer(b) + if accumulate { prior as i128 } else { 0 }).rem_euclid(modulus);
    assert_eq!(result as i128, expected);
    if bits < 32 {
        assert_eq!(result >> (bits * 2), 0);
    }
}

fn simd_reduce(source: u128, bits: u32, wide: bool, signed: bool, kind: u8) -> Option<u128> {
    if !matches!(bits, 8 | 16 | 32) || (bits == 32 && !wide) || kind > 3 {
        return None;
    }
    let element = |lane| {
        let raw = ((source >> (lane * bits)) as u64) & low_mask(bits);
        if signed {
            sign_extend(raw, bits)
        } else {
            raw as i64
        }
    };
    let mut result = element(0);
    for lane in 1..(if wide { 128 } else { 64 } / bits) {
        let value = element(lane);
        result = match kind {
            0 | 1 => result + value,
            2 => result.max(value),
            _ => result.min(value),
        };
    }
    let output_bits = if kind == 1 { bits * 2 } else { bits };
    Some(u128::from(result as u64 & low_mask(output_bits)))
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(17)]
fn integer_reductions_match_scalar_specification() {
    let source: u128 = kani::any();
    let size: u32 = kani::any();
    kani::assume(size <= 2);
    let bits = 8 << size;
    let wide: bool = kani::any();
    kani::assume(bits != 32 || wide);
    let kind: u8 = kani::any();
    kani::assume(kind <= 3);
    let signed: bool = kani::any();
    let result = simd_reduce(source, bits, wide, signed, kind).unwrap();
    let output_bits = if kind == 1 { bits * 2 } else { bits };
    assert!(result < (1u128 << output_bits));
    // Independent mathematical interpretation in a wider signed domain.
    // Min/max is specified by membership and ordering, not another min/max fold.
    let interpret = |raw: u128| -> i128 {
        if signed && raw & (1u128 << (bits - 1)) != 0 {
            raw as i128 - (1i128 << bits)
        } else {
            raw as i128
        }
    };
    let mut sum = 0i128;
    let mut member = false;
    for lane in 0..(if wide { 128 } else { 64 } / bits) {
        let raw = (source >> (lane * bits)) & ((1u128 << bits) - 1);
        let value = interpret(raw);
        sum += value;
        member |= result == raw;
        if kind == 2 {
            assert!(interpret(result) >= value);
        }
        if kind == 3 {
            assert!(interpret(result) <= value);
        }
    }
    if kind <= 1 {
        assert_eq!(result, (sum as u128) & ((1u128 << output_bits) - 1));
    } else {
        assert!(member);
    }
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

/// Number of bytes before the current translation granule ends.
/// Callers use 4 KiB or 16 KiB granules and scalar/SIMD structure widths <= 64.
fn page_fragment_len(address: u64, page_bytes: u64, bytes: usize) -> usize {
    bytes.min((page_bytes - (address & (page_bytes - 1))) as usize)
}

#[cfg(kani)]
#[kani::proof]
fn page_fragment_stays_within_translation_granule() {
    let address: u64 = kani::any();
    let page_bytes = if kani::any::<bool>() { 4096 } else { 16384 };
    let bytes: usize = kani::any();
    kani::assume(bytes > 0 && bytes <= 64);
    let first = page_fragment_len(address, page_bytes, bytes);
    let offset = address & (page_bytes - 1);
    assert!(first > 0 && first <= bytes);
    assert!(offset + first as u64 <= page_bytes);
    if first < bytes {
        assert!(offset + first as u64 == page_bytes);
        assert!(bytes - first < page_bytes as usize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::{GuestPageSize, HostPageSize};
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
        cpu.set_x(0, crate::bus::PL011_BASE);
        cpu.pc = 8;
        cpu.step().unwrap();
        assert_eq!(cpu.bus.console(), b"R");
    }

    fn discontiguous_pages(page: GuestPageSize, second_writable: bool) -> StaticCpu {
        let size = page.0 as u64;
        let mut memory =
            GuestMemory::allocate_on_host(page, relay_core::HostPageSize::SIXTEEN_KIB, 10 * size)
                .unwrap();
        for level in 0..3u64 {
            memory
                .write(level * size, &((level + 1) * size | 3).to_le_bytes())
                .unwrap();
        }
        // Adjacent virtual pages map to physical pages 5 and 7, with page 6
        // deliberately filled with poison to expose contiguous-physical bugs.
        memory
            .write(
                3 * size,
                &(5 * size | 3 | (1 << 10) | (1 << 6)).to_le_bytes(),
            )
            .unwrap();
        let permissions = if second_writable { 1 } else { 3 };
        memory
            .write(
                3 * size + 8,
                &(7 * size | 3 | (1 << 10) | (permissions << 6)).to_le_bytes(),
            )
            .unwrap();
        memory.write(5 * size, &vec![0x11; size as usize]).unwrap();
        memory.write(6 * size, &vec![0xcc; size as usize]).unwrap();
        memory.write(7 * size, &vec![0x22; size as usize]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.sysregs.sctlr_el1 |= 1;
        cpu.sysregs.tcr_el1 = 16 | (16 << 16);
        cpu.sysregs.current_el = 0;
        cpu
    }

    #[test]
    fn instruction_fetch_enforces_leaf_permissions_before_execution() {
        // Arm 102376, section 9.2: EL0 needs access and !UXN; EL1 needs
        // !PXN and must never execute an EL0-writable mapping. WXN also
        // disallows execution from mappings writable at the executing EL.
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let size = page.0 as u64;
            let mut cpu = discontiguous_pages(page, true);
            cpu.memory
                .write(5 * size + 0x100, &0xd503_201fu32.to_le_bytes())
                .unwrap();
            for (ap, el0_read, writable, el1_execute) in [
                (0u64, false, true, true),
                (1, true, true, false),
                (2, false, false, true),
                (3, true, false, true),
            ] {
                for el in [0, 1] {
                    for xn in 0..4u64 {
                        for wxn in [false, true] {
                            cpu.memory
                                .write(
                                    3 * size,
                                    &(5 * size | 3 | (1 << 10) | (ap << 6) | (xn << 53))
                                        .to_le_bytes(),
                                )
                                .unwrap();
                            cpu.pc = 0x100;
                            cpu.sysregs.current_el = el;
                            cpu.sysregs.sctlr_el1 = 1 | (u64::from(wxn) << 19);
                            cpu.sysregs.vbar_el1 = 0x8000;
                            cpu.sysregs.daif = 0;
                            cpu.nzcv = 0xa;
                            cpu.set_x(0, 0xfeed);
                            let allowed = if el == 0 {
                                el0_read && xn & 2 == 0
                            } else {
                                el1_execute && xn & 1 == 0
                            } && !(wxn && writable);
                            cpu.step().unwrap();
                            assert_eq!(cpu.x(0), 0xfeed);
                            if allowed {
                                assert_eq!(
                                    cpu.pc, 0x104,
                                    "page={} ap={ap} el={el} xn={xn} wxn={wxn}",
                                    page.0
                                );
                            } else {
                                assert_eq!(
                                    cpu.pc,
                                    if el == 0 { 0x8400 } else { 0x8200 },
                                    "page={} ap={ap} el={el} xn={xn} wxn={wxn}",
                                    page.0
                                );
                                assert_eq!(
                                    cpu.sysregs.esr_el1,
                                    if el == 0 { 0x8200_000f } else { 0x8600_000f }
                                );
                                assert_eq!(cpu.sysregs.elr_el1, 0x100);
                                assert_eq!(cpu.sysregs.far_el1, 0x100);
                                assert_eq!(
                                    cpu.sysregs.spsr_el1,
                                    0xa000_0000 | if el == 0 { 0 } else { 5 }
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn instruction_fetch_inherits_table_execute_restrictions() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let size = page.0 as u64;
            for table_level in 0..3u64 {
                for (bit, denied_el) in [(59, 1), (60, 0), (61, 0)] {
                    for el in [0, 1] {
                        let mut cpu = discontiguous_pages(page, true);
                        cpu.memory
                            .write(
                                3 * size,
                                &(5 * size | 3 | (1 << 10) | (3 << 6)).to_le_bytes(),
                            )
                            .unwrap();
                        cpu.memory
                            .write(
                                table_level * size,
                                &((table_level + 1) * size | 3 | (1u64 << bit)).to_le_bytes(),
                            )
                            .unwrap();
                        cpu.memory
                            .write(5 * size, &0xd503_201fu32.to_le_bytes())
                            .unwrap();
                        cpu.sysregs.current_el = el;
                        cpu.sysregs.vbar_el1 = 0x8000;
                        cpu.step().unwrap();
                        if el == denied_el {
                            assert_eq!(
                                cpu.pc,
                                if el == 0 { 0x8400 } else { 0x8200 },
                                "bit={bit}, level={table_level}"
                            );
                            assert_eq!(cpu.sysregs.esr_el1 & 0x3f, 0xf);
                        } else {
                            assert_eq!(cpu.pc, 4, "bit={bit}, level={table_level}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn instruction_fetch_reports_the_actual_translation_fault_level() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            for level in 0..4u64 {
                let mut cpu = discontiguous_pages(page, true);
                cpu.memory
                    .write(level * page.0 as u64, &0u64.to_le_bytes())
                    .unwrap();
                cpu.pc = 0x100;
                cpu.sysregs.vbar_el1 = 0x8000;
                cpu.step().unwrap();
                assert_eq!(cpu.pc, 0x8400);
                assert_eq!(cpu.sysregs.esr_el1, 0x8200_0004 | level);
                assert_eq!(cpu.sysregs.far_el1, 0x100);
                assert_eq!(cpu.sysregs.elr_el1, 0x100);
            }
        }
    }

    #[test]
    fn structure_transfers_cross_discontiguous_pages_and_wrap_registers() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let size = u64::from(page.0);
            for split in 1..64usize {
                for interleaved in [false, true] {
                    for load in [false, true] {
                        let mut cpu = discontiguous_pages(page, true);
                        let word: u32 = (if load { 0x4cdf_23ff } else { 0x4c9f_23ff })
                            & if interleaved { !0x2000 } else { u32::MAX };
                        cpu.memory.write(5 * size, &word.to_le_bytes()).unwrap();
                        let raw: [u8; 64] = std::array::from_fn(|i| (i as u8).wrapping_mul(37));
                        let expected: [[u8; 16]; 4] = std::array::from_fn(|register| {
                            std::array::from_fn(|byte| {
                                raw[if interleaved {
                                    byte * 4 + register
                                } else {
                                    register * 16 + byte
                                }]
                            })
                        });
                        if load {
                            cpu.memory
                                .write(6 * size - split as u64, &raw[..split])
                                .unwrap();
                            cpu.memory.write(7 * size, &raw[split..]).unwrap();
                        } else {
                            for index in 0..4 {
                                cpu.v[(31 + index) & 31] = u128::from_le_bytes(expected[index]);
                            }
                        }
                        cpu.sp = size - split as u64;
                        cpu.step().unwrap();
                        assert_eq!(cpu.pc, 4);
                        assert_eq!(cpu.sp, size - split as u64 + 64);
                        if load {
                            for index in 0..4 {
                                assert_eq!(cpu.v[(31 + index) & 31].to_le_bytes(), expected[index]);
                            }
                        } else {
                            let mut actual = [0; 64];
                            cpu.memory
                                .read(6 * size - split as u64, &mut actual[..split])
                                .unwrap();
                            cpu.memory.read(7 * size, &mut actual[split..]).unwrap();
                            assert_eq!(actual, raw);
                        }
                        let mut poison = [0; 64];
                        cpu.memory.read(6 * size, &mut poison).unwrap();
                        assert_eq!(poison, [0xcc; 64]);
                    }
                }
            }
        }
    }

    #[test]
    fn structure_store_fault_has_no_partial_write_or_writeback() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            for word in [0x4c9f_23ffu32, 0x4c9f_03ff] {
                let size = u64::from(page.0);
                let mut cpu = discontiguous_pages(page, false);
                cpu.memory.write(5 * size, &word.to_le_bytes()).unwrap();
                cpu.v = [u128::MAX; 32];
                cpu.sp = size - 32;
                cpu.step().unwrap();
                assert_eq!(cpu.sysregs.esr_el1, 0x9200_004f);
                assert_eq!(cpu.sysregs.far_el1, size);
                assert_eq!(cpu.sysregs.sp_el0, size - 32);
                let mut before = [0; 32];
                cpu.memory.read(6 * size - 32, &mut before).unwrap();
                assert_eq!(before, [0x11; 32]);
                assert_eq!(cpu.v, [u128::MAX; 32]);
            }
        }
    }

    #[test]
    fn instruction_fetch_access_flag_fault_keeps_instruction_unexecuted() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            for el in [0, 1] {
                let size = u64::from(page.0);
                let mut cpu = discontiguous_pages(page, true);
                cpu.memory
                    .write(3 * size, &(5 * size | 3 | (3 << 6)).to_le_bytes())
                    .unwrap();
                cpu.memory
                    .write(5 * size, &0xd280_00e0u32.to_le_bytes())
                    .unwrap(); // MOV X0,#7
                cpu.sysregs.current_el = el;
                cpu.sysregs.vbar_el1 = 0x8000;
                cpu.x[0] = 99;
                cpu.step().unwrap();
                assert_eq!(cpu.x[0], 99);
                assert_eq!(cpu.sysregs.elr_el1, 0);
                assert_eq!(cpu.sysregs.far_el1, 0);
                assert_eq!(
                    cpu.sysregs.esr_el1,
                    if el == 0 { 0x8200_000b } else { 0x8600_000b }
                );
            }
        }
    }

    #[test]
    fn data_access_flag_fault_preserves_memory_and_write_syndrome() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let size = u64::from(page.0);
            let mut cpu = discontiguous_pages(page, true);
            cpu.memory
                .write(3 * size, &(5 * size | 3 | (1 << 6)).to_le_bytes())
                .unwrap();
            assert!(cpu.read8(0x123).is_err());
            assert_eq!(cpu.pending_data_abort.get(), Some((0x123, false, 0xb)));
            assert!(cpu.write8(0x123, 0xff).is_err());
            assert_eq!(cpu.pending_data_abort.get(), Some((0x123, true, 0xb)));
            let mut byte = [0];
            cpu.memory.read(5 * size + 0x123, &mut byte).unwrap();
            assert_eq!(byte, [0x11]);
        }
    }

    #[test]
    fn instruction_fetch_preserves_configuration_errors_and_external_faults() {
        let mut cpu = discontiguous_pages(GuestPageSize::FOUR_KIB, true);
        cpu.sysregs.tcr_el1 = 0; // 64-bit VAs are outside the declared profile.
        assert!(cpu
            .step()
            .unwrap_err()
            .to_string()
            .contains("virtual address size"));
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.abort_count, 0);
        cpu.sysregs.tcr_el1 = 16;
        cpu.sysregs.vbar_el1 = 0x8000;
        cpu.memory
            .write(0, &(0x100000u64 | 3).to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.sysregs.esr_el1, 0x8200_0015); // external abort, level-1 walk
        assert_eq!(cpu.sysregs.far_el1, 0);
        cpu.pc = 0x100000;
        cpu.sysregs.current_el = 0;
        cpu.sysregs.sctlr_el1 = 0;
        cpu.step().unwrap();
        assert_eq!(cpu.sysregs.esr_el1, 0x8200_0010); // external abort on fetch
        assert_eq!(cpu.sysregs.far_el1, 0x100000);
    }

    #[test]
    fn instruction_fetch_abort_from_el1_sp0_saves_mode_and_switches_stack() {
        let mut cpu = discontiguous_pages(GuestPageSize::FOUR_KIB, true);
        cpu.memory
            .write(
                0x3000,
                &(0x5000u64 | 3 | (1 << 10) | (1 << 53)).to_le_bytes(),
            )
            .unwrap();
        cpu.sysregs.current_el = 1;
        cpu.sysregs.spsel = 0;
        cpu.sysregs.vbar_el1 = 0x8000;
        cpu.sysregs.daif = 0;
        cpu.sp = 0x6000;
        cpu.sp_el1 = 0x9000;
        cpu.exclusive = Some((0x100, 8));
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x8000);
        assert_eq!(cpu.sysregs.esr_el1, 0x8600_000f);
        assert_eq!(cpu.sysregs.spsr_el1, 4);
        assert_eq!(cpu.sysregs.sp_el0, 0x6000);
        assert_eq!(cpu.sp, 0x9000);
        assert_eq!(cpu.sysregs.spsel, 1);
        assert_eq!(cpu.exclusive, None);
    }

    #[test]
    fn table_permissions_restrict_data_without_treating_execute_never_as_read_never() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            for level in 0..3u64 {
                let size = page.0 as u64;
                let mut cpu = discontiguous_pages(page, true);
                let next = (level + 1) * size | 3;
                cpu.memory
                    .write(level * size, &(next | (1 << 62) | (3 << 59)).to_le_bytes())
                    .unwrap();
                assert_eq!(cpu.read8(0).unwrap(), 0x11);
                assert!(cpu.write8(0, 0xee).is_err());
                assert_eq!(cpu.pending_data_abort.get(), Some((0, true, 0xf)));
                assert_eq!(cpu.read8(0).unwrap(), 0x11);
                cpu.memory
                    .write(level * size, &(next | (1 << 61)).to_le_bytes())
                    .unwrap();
                assert!(cpu.read8(0).is_err());
                assert_eq!(cpu.pending_data_abort.get(), Some((0, false, 0xf)));
                cpu.sysregs.current_el = 1;
                assert_eq!(cpu.read8(0).unwrap(), 0x11);
            }
        }
    }

    #[test]
    fn sixteen_kib_walk_uses_tcr_start_level_for_each_address_half() {
        let page = GuestPageSize::SIXTEEN_KIB;
        let size = page.0 as u64;
        for va in [0x123u64, 0xffff_c000_0000_0123] {
            let mut memory = GuestMemory::allocate_on_host(
                page,
                relay_core::HostPageSize::SIXTEEN_KIB,
                8 * size,
            )
            .unwrap();
            for (level, shift) in [36, 25, 14].into_iter().enumerate() {
                let index = ((va & ((1u64 << 47) - 1)) >> shift) & 2047;
                let next = if level == 2 {
                    7 * size
                } else {
                    (level as u64 + 1) * size
                };
                memory
                    .write(
                        level as u64 * size + index * 8,
                        &(next | 3 | (1 << 10)).to_le_bytes(),
                    )
                    .unwrap();
            }
            let mut cpu = StaticCpu::new(memory, va).unwrap();
            cpu.sysregs.sctlr_el1 = 1;
            // The unselected half retains 48 VA bits. The selected half has
            // 47 VA bits, so its 16 KiB walk starts at level one.
            cpu.sysregs.tcr_el1 = if va >> 63 == 0 {
                17 | (16 << 16)
            } else {
                16 | (17 << 16)
            };
            assert_eq!(cpu.physical(va).unwrap(), 7 * size + 0x123);
            assert_eq!(cpu.data_physical(va, true).unwrap(), 7 * size + 0x123);
        }
    }

    #[test]
    fn unaligned_loads_translate_both_pages() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let cpu = discontiguous_pages(page, true);
            let edge = page.0 as u64;
            assert_eq!(cpu.read16(edge - 1).unwrap(), 0x2211);
            assert_eq!(cpu.read32(edge - 2).unwrap(), 0x2222_1111);
            assert_eq!(cpu.read64(edge - 4).unwrap(), 0x2222_2222_1111_1111);
            assert_eq!(
                cpu.read_vector(edge - 8, 16).unwrap(),
                0x2222_2222_2222_2222_1111_1111_1111_1111
            );
        }
    }

    #[test]
    fn unaligned_stores_translate_both_pages_without_touching_neighbor() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let edge = page.0 as u64;
            for width in [2, 4, 8, 16] {
                let mut cpu = discontiguous_pages(page, true);
                let address = edge - width / 2;
                match width {
                    2 => cpu.write16(address, 0xffff).unwrap(),
                    4 => cpu.write32(address, u32::MAX).unwrap(),
                    8 => cpu.write64(address, u64::MAX).unwrap(),
                    16 => cpu.write_vector(address, 16, u128::MAX).unwrap(),
                    _ => unreachable!(),
                }
                let mut bytes = [0; 8];
                let count = width as usize / 2;
                cpu.memory.read(7 * edge, &mut bytes[..count]).unwrap();
                assert_eq!(&bytes[..count], &vec![0xff; count]);
                cpu.memory.read(6 * edge, &mut bytes).unwrap();
                assert_eq!(bytes, [0xcc; 8]);
            }
        }
    }

    #[cfg(all(target_arch = "aarch64", not(miri)))]
    #[test]
    fn cross_page_simd_load_matches_native_aarch64_at_every_split() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            for split in 1..16usize {
                let mut cpu = discontiguous_pages(page, true);
                let edge = page.0 as u64;
                let expected: [u8; 16] = std::array::from_fn(|index| (index * 13 + split) as u8);
                cpu.memory
                    .write(6 * edge - split as u64, &expected[..split])
                    .unwrap();
                cpu.memory.write(7 * edge, &expected[split..]).unwrap();
                let mut native = [0u8; 16];
                // SAFETY: both arrays cover the full 16-byte access. Q0 is
                // declared clobbered; no guest code executes on the host.
                unsafe {
                    std::arch::asm!(
                        "ldr q0, [{source}]",
                        "str q0, [{destination}]",
                        source = in(reg) expected.as_ptr(),
                        destination = in(reg) native.as_mut_ptr(),
                        out("v0") _,
                        options(nostack, preserves_flags),
                    );
                }
                assert_eq!(
                    cpu.read_vector(edge - split as u64, 16)
                        .unwrap()
                        .to_le_bytes(),
                    native
                );
            }
        }
    }

    #[test]
    fn unaligned_store_checks_second_page_permissions() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let mut cpu = discontiguous_pages(page, false);
            let edge = page.0 as u64;
            assert!(cpu.write64(edge - 4, u64::MAX).is_err());
            assert_eq!(cpu.pending_data_abort.get(), Some((edge, true, 0x0f)));
            let mut bytes = [0; 8];
            cpu.memory.read(6 * edge - 4, &mut bytes[..4]).unwrap();
            assert_eq!(&bytes[..4], &[0x11; 4]);
            cpu.memory.read(7 * edge, &mut bytes).unwrap();
            assert_eq!(bytes, [0x22; 8]);
        }
    }

    #[test]
    fn loads_bytes_with_unsigned_offsets() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x3973_0000u32.to_le_bytes()).unwrap(); // LDRB W0,[X0,#3264]
        memory.write(4, &0xf980_00d1u32.to_le_bytes()).unwrap(); // PRFM PSTL1STRM,[X6]
        memory.write(3264, &[0xa5]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 0xa5);
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 8);
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
    fn unprivileged_store_and_load_do_not_update_base() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xf800_081fu32.to_le_bytes()).unwrap(); // STTR XZR,[X0]
        memory.write(4, &0xf840_0801u32.to_le_bytes()).unwrap(); // LDTR X1,[X0]
        memory.write(0x100, &u64::MAX.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x100);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 0x100);
        assert_eq!(cpu.x(1), 0);
    }

    #[test]
    fn unprivileged_translation_fault_enters_el1_data_abort_vector() {
        let memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.sysregs.vbar_el1 = 0x800;
        cpu.sysregs.daif = 0;
        cpu.nzcv = 0xa;

        cpu.enter_data_abort(0xaaaa_0000_1000, true, 0x07);

        assert_eq!(cpu.pc, 0xa00);
        assert_eq!(cpu.sysregs.elr_el1, 0);
        assert_eq!(cpu.sysregs.far_el1, 0xaaaa_0000_1000);
        assert_eq!(cpu.sysregs.esr_el1, 0x9600_0047);
        assert_eq!(cpu.sysregs.spsr_el1, 0xa000_0005);
        assert_eq!(cpu.sysregs.daif, 0x3c0);
    }

    #[test]
    fn el0_instruction_translation_fault_enters_lower_el_sync_vector() {
        let memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        let mut cpu = StaticCpu::new(memory, 0xffff_99ce_dd40).unwrap();
        cpu.sysregs.vbar_el1 = 0xffff_ffff_ff47_0000;
        cpu.sysregs.current_el = 0;
        cpu.sysregs.daif = 0;
        cpu.nzcv = 5;
        cpu.sp = 0x7000;
        cpu.sp_el1 = 0x9000;

        cpu.enter_instruction_abort(cpu.pc, 0x07);

        assert_eq!(cpu.pc, 0xffff_ffff_ff47_0400);
        assert_eq!(cpu.sysregs.elr_el1, 0xffff_99ce_dd40);
        assert_eq!(cpu.sysregs.far_el1, 0xffff_99ce_dd40);
        assert_eq!(cpu.sysregs.esr_el1, 0x8200_0007);
        assert_eq!(cpu.sysregs.spsr_el1, 0x5000_0000);
        assert_eq!(cpu.sysregs.current_el, 1);
        assert_eq!(cpu.sysregs.daif, 0x3c0);
        assert_eq!(cpu.sysregs.sp_el0, 0x7000);
        assert_eq!(cpu.sp, 0x9000);
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
    fn logical_register_rotate_executes_kernel_xor() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xcac5_c085u32.to_le_bytes()).unwrap(); // EOR X5,X4,X5,ROR #48
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(4, 0x00ff_00ff_00ff_00ff);
        cpu.set_x(5, 0x1122_3344_5566_7788);
        cpu.step().unwrap();
        assert_eq!(cpu.x(5), 0x00ff_00ff_00ff_00ff ^ 0x3344_5566_7788_1122);
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
            (20, 0xd508_711f_u32), // IC IALLUIS
            (24, 0xd50b_7b22_u32), // DC CVAU,X2
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
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 28);
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
    fn divides_linux_geometry_words_and_handles_zero_divisors() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x1ad6_0c21u32.to_le_bytes()).unwrap(); // UDIV W1,W1,W22
        memory.write(4, &0x1ac3_0c22u32.to_le_bytes()).unwrap(); // SDIV W2,W1,W3
        memory.write(8, &0x9ac3_0824u32.to_le_bytes()).unwrap(); // UDIV X4,X1,X3
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 100);
        cpu.set_x(22, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.x(1), 25);
        cpu.set_x(1, (-100_i32) as u32 as u64);
        cpu.set_x(3, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.x(2), (-25_i32) as u32 as u64);
        cpu.set_x(1, 99);
        cpu.set_x(3, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.x(4), 0);
    }

    #[test]
    fn signed_long_multiply_extends_32_bit_operands() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x9b26_7f46u32.to_le_bytes()).unwrap(); // SMULL X6,W26,W6
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(26, u32::MAX as u64);
        cpu.set_x(6, 7);
        cpu.step().unwrap();
        assert_eq!(cpu.x(6), (-7_i64) as u64);
    }

    #[test]
    fn high_multiply_returns_upper_product_word() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x9bc1_7c42u32.to_le_bytes()).unwrap(); // UMULH X2,X2,X1
        memory.write(4, &0x9b41_7c42u32.to_le_bytes()).unwrap(); // SMULH X2,X2,X1
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, u64::MAX);
        cpu.set_x(2, u64::MAX);
        cpu.step().unwrap();
        assert_eq!(cpu.x(2), u64::MAX - 1);
        cpu.set_x(1, 2);
        cpu.set_x(2, (-3_i64) as u64);
        cpu.step().unwrap();
        assert_eq!(cpu.x(2), u64::MAX);
    }

    #[test]
    fn extract_supports_rotate_immediate_alias() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x1386_40c6u32.to_le_bytes()).unwrap(); // ROR W6,W6,#16
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(6, 0x1234_5678);
        cpu.step().unwrap();
        assert_eq!(cpu.x(6), 0x5678_1234);
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
    fn eret_to_el0_selects_user_stack_and_saves_el1_stack() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd69f_03e0u32.to_le_bytes()).unwrap(); // ERET
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.sp = 0x9000;
        cpu.sysregs.sp_el0 = 0x7000;
        cpu.sysregs.elr_el1 = 0x100;
        cpu.sysregs.spsr_el1 = 0;

        cpu.step().unwrap();

        assert_eq!(cpu.sysregs.current_el, 0);
        assert_eq!(cpu.sp_el1, 0x9000);
        assert_eq!(cpu.sp, 0x7000);
    }

    #[test]
    fn hvc_exposes_single_vcpu_psci_0_2() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd400_0002u32.to_le_bytes()).unwrap(); // HVC #0
        memory.write(4, &0xd400_0002u32.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x8400_0000);
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 2);
        cpu.set_x(0, 0x8400_0003); // CPU_ON is unavailable with one vCPU.
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), u64::MAX);
    }

    #[test]
    fn exception_return_restores_irq_mask_from_spsr() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd69f_03e0u32.to_le_bytes()).unwrap(); // ERET
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.sysregs.elr_el1 = 0x100;
        cpu.sysregs.spsr_el1 = 0xa000_0340;
        cpu.sysregs.daif = 0x3c0;
        cpu.nzcv = 4;
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0x100);
        assert_eq!(cpu.sysregs.daif, 0x340);
        assert_eq!(cpu.nzcv, 0xa);
    }

    #[test]
    fn irq_entry_preserves_condition_flags_in_spsr() {
        let memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        let mut cpu = StaticCpu::new(memory, 0x100).unwrap();
        cpu.nzcv = 0xb;
        cpu.exclusive = Some((0x200, 4));
        cpu.sysregs.daif = 0;
        cpu.sysregs.write(crate::sysregs::CNTP_CVAL_EL0, 0).unwrap();
        cpu.sysregs.write(crate::sysregs::CNTP_CTL_EL0, 1).unwrap();
        cpu.take_pending_irq().unwrap();
        assert_eq!(cpu.sysregs.elr_el1, 0x100);
        assert_eq!(cpu.sysregs.spsr_el1 >> 28, 0xb);
        assert_eq!(cpu.sysregs.daif & 0x80, 0x80);
        assert_eq!(cpu.exclusive, None);
    }

    #[test]
    fn daif_immediates_preserve_interrupt_mask_state() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd503_42ffu32.to_le_bytes()).unwrap(); // MSR DAIFClr,#2
        memory.write(4, &0xd503_42dfu32.to_le_bytes()).unwrap(); // MSR DAIFSet,#2
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.sysregs.daif & 0x80, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.sysregs.daif & 0x80, 0x80);
    }

    #[test]
    fn el0_svc_enters_lower_sync_vector_and_returns_after_instruction() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xd402_4681u32.to_le_bytes()).unwrap(); // SVC #0x1234
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.sysregs.current_el = 0;
        cpu.sysregs.vbar_el1 = 0x800;
        cpu.sp = 0x700;
        cpu.sp_el1 = 0x600;
        cpu.step().unwrap();
        assert_eq!(cpu.pc, 0xc00);
        assert_eq!(cpu.sysregs.elr_el1, 4);
        assert_eq!(cpu.sysregs.esr_el1, 0x5600_1234);
        assert_eq!(cpu.sysregs.sp_el0, 0x700);
        assert_eq!(cpu.sp, 0x600);
    }

    #[test]
    fn ngc_converts_carry_into_all_zero_or_all_one_mask() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xda1f_03e0u32.to_le_bytes()).unwrap(); // NGC X0,XZR
        memory.write(4, &0xda1f_03e1u32.to_le_bytes()).unwrap(); // NGC X1,XZR
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.nzcv = 2;
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 0);
        cpu.nzcv = 0;
        cpu.step().unwrap();
        assert_eq!(cpu.x(1), u64::MAX);
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
    fn word_pair_store_uses_four_byte_elements() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x2908_7fe1u32.to_le_bytes()).unwrap(); // STP W1,WZR,[SP,#64]
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.sp = 0x100;
        cpu.set_x(1, 0xfeed_face);
        cpu.step().unwrap();
        let mut stored = [0xff; 8];
        cpu.memory.read(0x140, &mut stored).unwrap();
        assert_eq!(&stored[..4], &0xfeed_faceu32.to_le_bytes());
        assert_eq!(&stored[4..], &[0; 4]);
    }

    #[test]
    fn non_temporal_pair_store_uses_signed_offset_without_writeback() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xa830_0c02u32.to_le_bytes()).unwrap(); // STNP X2,X3,[X0,#-256]
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x300);
        cpu.set_x(2, 0x1122_3344_5566_7788);
        cpu.set_x(3, 0x99aa_bbcc_ddee_ff00);

        cpu.step().unwrap();

        assert_eq!(cpu.x(0), 0x300);
        assert_eq!(cpu.read64(0x200).unwrap(), 0x1122_3344_5566_7788);
        assert_eq!(cpu.read64(0x208).unwrap(), 0x99aa_bbcc_ddee_ff00);
    }

    #[test]
    fn simd_pair_load_reads_two_q_registers_without_writeback() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xad40_0420u32.to_le_bytes()).unwrap(); // LDP Q0,Q1,[X1]
        memory.write(0x200, &[0x11; 16]).unwrap();
        memory.write(0x210, &[0x22; 16]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0x200);

        cpu.step().unwrap();

        assert_eq!(cpu.x(1), 0x200);
        assert_eq!(cpu.v[0], u128::from_le_bytes([0x11; 16]));
        assert_eq!(cpu.v[1], u128::from_le_bytes([0x22; 16]));
    }

    #[test]
    fn simd_ld1_two_vectors_loads_consecutive_bytes_without_writeback() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x4c40_a021u32.to_le_bytes()).unwrap(); // LD1 {V1.16B,V2.16B},[X1]
        let first = 0x0f0e_0d0c_0b0a_0908_0706_0504_0302_0100u128;
        let second = 0x1f1e_1d1c_1b1a_1918_1716_1514_1312_1110u128;
        memory.write(0x100, &first.to_le_bytes()).unwrap();
        memory.write(0x110, &second.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0x100);
        cpu.step().unwrap();
        assert_eq!(cpu.v[1], first);
        assert_eq!(cpu.v[2], second);
        assert_eq!(cpu.x(1), 0x100);
    }

    #[test]
    fn simd_dup_broadcasts_general_register_word() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x4e04_0c40u32.to_le_bytes()).unwrap(); // DUP V0.4S,W2
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(2, 0xfeed_beef);
        cpu.step().unwrap();
        assert_eq!(cpu.v[0], 0xfeed_beef_feed_beef_feed_beef_feed_beefu128);
    }

    #[test]
    fn simd_and_combines_full_vectors() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x4e20_1c21u32.to_le_bytes()).unwrap(); // AND V1.16B,V1.16B,V0.16B
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[0] = 0xff00_ff00_ff00_ff00_ff00_ff00_ff00_ff00;
        cpu.v[1] = 0x0ff0_0ff0_0ff0_0ff0_0ff0_0ff0_0ff0_0ff0;
        cpu.step().unwrap();
        assert_eq!(cpu.v[1], 0x0f00_0f00_0f00_0f00_0f00_0f00_0f00_0f00);
    }

    #[test]
    fn simd_ld1r_and_eor_decode_glibc_pointer_table() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x4d40_cc3fu32.to_le_bytes()).unwrap(); // LD1R {V31.2D},[X1]
        memory.write(4, &0x6e3f_1fdeu32.to_le_bytes()).unwrap(); // EOR V30.16B,V30.16B,V31.16B
        memory
            .write(0x100, &0x1122_3344_5566_7788u64.to_le_bytes())
            .unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0x100);
        cpu.v[30] = 0xffff_ffff_ffff_ffff_0000_0000_0000_0000;
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], 0x1122_3344_5566_7788_1122_3344_5566_7788);
        cpu.step().unwrap();
        assert_eq!(cpu.v[30], 0xeedd_ccbb_aa99_8877_1122_3344_5566_7788);
    }

    #[test]
    fn glibc_strlen_page_end_sequence_returns_exact_length() {
        let instructions: [u32; 19] = [
            0x927b_e801,
            0x5281_8062,
            0x72b8_0602,
            0x4c40_a021,
            0x4e04_0c40,
            0x4e20_9821,
            0x4e20_9842,
            0x4e20_1c21,
            0x4e20_1c42,
            0x4e22_bc20,
            0x4e20_bc00,
            0x9e66_0003,
            0xd37f_f804,
            0x9ac4_2463,
            0xb4ff_fc23,
            0xdac0_0063,
            0xdac0_1060,
            0xd341_fc00,
            0xd65f_03c0,
        ];
        for (start, length) in [(0x1fe1u64, 3usize), (0x1ff0, 15), (0x1fff, 0)] {
            let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 0x3000).unwrap();
            for (index, instruction) in instructions.into_iter().enumerate() {
                memory
                    .write((index * 4) as u64, &instruction.to_le_bytes())
                    .unwrap();
            }
            memory.write(start, &vec![b'x'; length]).unwrap();
            memory.write(start + length as u64, &[0]).unwrap();
            let mut cpu = StaticCpu::new(memory, 0).unwrap();
            cpu.set_x(0, start);
            cpu.set_x(30, 0x200);
            for _ in 0..instructions.len() {
                cpu.step().unwrap();
                if cpu.pc == 0x200 {
                    break;
                }
            }
            assert_eq!(cpu.pc, 0x200);
            assert_eq!(cpu.x(0), length as u64, "start={start:#x}");
        }
    }

    #[test]
    fn glibc_strlen_short_sequence_returns_exact_length() {
        let instructions: [u32; 21] = [
            0xd503_245f,
            0x9240_2c04,
            0xf13f_809f,
            0x5400_06c8,
            0xa940_0c02,
            0xb200_c3e8,
            0xcb08_0044,
            0xb200_d845,
            0xcb08_0066,
            0xb200_d867,
            0xea25_0084,
            0x8a27_00c5,
            0xfa40_08a0,
            0x5400_0100,
            0x9a85_3084,
            0xd280_0100,
            0xdac0_0c84,
            0x9a80_33e0,
            0xdac0_1084,
            0x8b44_0c00,
            0xd65f_03c0,
        ];
        for length in 0..16usize {
            let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
            for (index, instruction) in instructions.into_iter().enumerate() {
                memory
                    .write((index * 4) as u64, &instruction.to_le_bytes())
                    .unwrap();
            }
            memory.write(0x200, &vec![b'x'; length]).unwrap();
            memory.write(0x200 + length as u64, &[0]).unwrap();
            let mut cpu = StaticCpu::new(memory, 0).unwrap();
            cpu.set_x(0, 0x200);
            cpu.set_x(30, 0x180);
            for _ in 0..instructions.len() {
                cpu.step().unwrap();
                if cpu.pc == 0x180 {
                    break;
                }
            }
            assert_eq!(cpu.pc, 0x180);
            assert_eq!(cpu.x(0), length as u64, "length={length}");
        }
    }

    #[test]
    fn glibc_strlen_complete_sequence_matches_scalar_lengths() {
        let instructions: [u32; 76] = [
            0xd503245f, 0x92402c04, 0xf13f809f, 0x540006c8, 0xa9400c02, 0xb200c3e8, 0xcb080044,
            0xb200d845, 0xcb080066, 0xb200d867, 0xea250084, 0x8a2700c5, 0xfa4008a0, 0x54000100,
            0x9a853084, 0xd2800100, 0xdac00c84, 0x9a8033e0, 0xdac01084, 0x8b440c00, 0xd65f03c0,
            0xa9410c02, 0xcb080044, 0xb200d845, 0xcb080066, 0xb200d867, 0xea250084, 0x8a2700c5,
            0xfa4008a0, 0x54000140, 0x9a853084, 0xd2800300, 0xdac00c84, 0xd2800206, 0xdac01084,
            0x9a8030c0, 0x8b440c00, 0xd65f03c0, 0xd503201f, 0x927be801, 0xadc10821, 0x6e22ac20,
            0x6e20ac00, 0x0e209800, 0x9e660003, 0xb4ffff63, 0x4e209820, 0xcb000020, 0x35000063,
            0x4e209840, 0x91004000, 0x0f0c8400, 0x9e660003, 0xdac00063, 0xdac01062, 0x8b420800,
            0xd65f03c0, 0x927be801, 0x52818062, 0x72b80602, 0x4c40a021, 0x4e040c40, 0x4e209821,
            0x4e209842, 0x4e201c21, 0x4e201c42, 0x4e22bc20, 0x4e20bc00, 0x9e660003, 0xd37ff804,
            0x9ac42463, 0xb4fffc23, 0xdac00063, 0xdac01060, 0xd341fc00, 0xd65f03c0,
        ];
        for (start, length) in [
            (0x800u64, 0usize),
            (0x801, 7),
            (0x80f, 16),
            (0x810, 31),
            (0x811, 63),
            (0x1fe1, 3),
            (0x1ff0, 15),
            (0x1fff, 32),
        ] {
            let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 0x3000).unwrap();
            for (index, instruction) in instructions.into_iter().enumerate() {
                memory
                    .write((index * 4) as u64, &instruction.to_le_bytes())
                    .unwrap();
            }
            memory.write(start, &vec![b'x'; length]).unwrap();
            memory.write(start + length as u64, &[0]).unwrap();
            let mut cpu = StaticCpu::new(memory, 0).unwrap();
            cpu.set_x(0, start);
            cpu.set_x(30, 0x400);
            for _ in 0..256 {
                cpu.step().unwrap();
                if cpu.pc == 0x400 {
                    break;
                }
            }
            assert_eq!(cpu.pc, 0x400, "start={start:#x} length={length}");
            assert_eq!(cpu.x(0), length as u64, "start={start:#x}");
        }
    }

    #[test]
    fn glibc_strcmp_sequence_matches_byte_ordering() {
        let instructions: [u32; 77] = [
            0xd503245f, 0xcb00002a, 0xb200c3e8, 0x92400806, 0xf240095f, 0x54000401, 0xb50002c6,
            0xd503201f, 0xf86a6803, 0xf8408402, 0xcb080044, 0xb200d846, 0xea260084, 0xfa430040,
            0x54ffff40, 0xca030045, 0xaa0400a6, 0xdac00cc6, 0xdac00c42, 0xdac00c63, 0xdac010c9,
            0x9ac92042, 0x9ac92063, 0xd378fc42, 0xcb43e040, 0xd65f03c0, 0xd503201f, 0xd503201f,
            0x927df000, 0xf86a6803, 0xf8408402, 0xcb010fe9, 0x92800006, 0x9ac924c6, 0xaa060042,
            0xaa060063, 0x17ffffe6, 0xb4000106, 0x38401402, 0x38401423, 0x7100005f, 0x7a431040,
            0x54000421, 0xf240081f, 0x54ffff41, 0xcb010fe9, 0x927df021, 0xf8408427, 0x9ac92506,
            0xaa0600e7, 0xcb0800e4, 0xb200d8e6, 0xea260084, 0x540001e1, 0xcb000025, 0xd503201f,
            0xf8656807, 0xf86a6803, 0xcb0800e4, 0xb200d8e6, 0xf8408402, 0xea260084, 0xfa430040,
            0x54ffff20, 0x9ac92086, 0xca030045, 0xaa0600a6, 0xb5fff9c6, 0xf9400002, 0xcb0903e9,
            0x9ac924e3, 0x9ac92484, 0xca030045, 0xaa0400a6, 0x17ffffc7, 0xcb030040, 0xd65f03c0,
        ];
        let cases: &[(&[u8], &[u8])] = &[
            (
                b"rl_completion_append_character",
                b"rl_completion_append_character",
            ),
            (
                b"rl_completion_append_character",
                b"rl_completion_append_charactes",
            ),
            (b"abc", b"abd"),
            (b"abd", b"abc"),
            (b"", b"a"),
        ];
        for (index, (left, right)) in cases.iter().enumerate() {
            for left_alignment in 0..8u64 {
                for right_alignment in 0..8u64 {
                    let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
                    for (offset, instruction) in instructions.into_iter().enumerate() {
                        memory
                            .write((offset * 4) as u64, &instruction.to_le_bytes())
                            .unwrap();
                    }
                    let left_address = 0x800 + left_alignment;
                    let right_address = 0x900 + right_alignment;
                    memory.write(left_address, left).unwrap();
                    memory
                        .write(left_address + left.len() as u64, &[0])
                        .unwrap();
                    memory.write(right_address, right).unwrap();
                    memory
                        .write(right_address + right.len() as u64, &[0])
                        .unwrap();
                    let mut cpu = StaticCpu::new(memory, 0).unwrap();
                    cpu.set_x(0, left_address);
                    cpu.set_x(1, right_address);
                    cpu.set_x(30, 0x400);
                    for _ in 0..256 {
                        cpu.step().unwrap();
                        if cpu.pc == 0x400 {
                            break;
                        }
                    }
                    assert_eq!(cpu.pc, 0x400);
                    let expected = left.cmp(right);
                    let actual = (cpu.x(0) as i64).cmp(&0);
                    assert_eq!(
                actual, expected,
                "case={index} left-alignment={left_alignment} right-alignment={right_alignment}"
            );
                }
            }
        }
    }

    #[test]
    fn simd_movi_zero_clears_full_vector_register() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x4f00_041fu32.to_le_bytes()).unwrap(); // MOVI V31.4S,#0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[31] = u128::MAX;

        cpu.step().unwrap();

        assert_eq!(cpu.v[31], 0);
        assert_eq!(cpu.pc, 4);
    }

    #[test]
    fn simd_movi_two_words_clears_upper_half() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x0f00_043fu32.to_le_bytes()).unwrap(); // MOVI V31.2S,#1
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[31] = u128::MAX;
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], 0x0000_0001_0000_0001);
    }

    #[test]
    fn simd_movi_eight_bytes_clears_upper_half() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x0f06_e7efu32.to_le_bytes()).unwrap(); // MOVI V15.8B,#0xdf
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[15] = u128::MAX;
        cpu.step().unwrap();
        assert_eq!(cpu.v[15], 0xdfdf_dfdf_dfdf_dfdf);
    }

    #[test]
    fn simd_movi_bitmask_expands_bits_to_bytes() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x2f00_e5ffu32.to_le_bytes()).unwrap(); // MOVI D31,#0x00000000ffffffff
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[31] = u128::MAX;
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], 0x0000_0000_ffff_ffff);
    }

    #[test]
    fn simd_mvni_zero_sets_full_vector_register() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x6f00_041fu32.to_le_bytes()).unwrap(); // MVNI V31.4S,#0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], u128::MAX);
    }

    #[test]
    fn simd_mvni_two_words_clears_upper_half() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x2f00_041fu32.to_le_bytes()).unwrap(); // MVNI V31.2S,#0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[31] = u128::MAX;
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], u128::from(u64::MAX));
    }

    #[test]
    fn simd_dup_broadcasts_general_register_byte() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x4e01_0c20u32.to_le_bytes()).unwrap(); // DUP V0.16B,W1
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0xa5);

        cpu.step().unwrap();

        assert_eq!(cpu.v[0], 0xa5a5_a5a5_a5a5_a5a5_a5a5_a5a5_a5a5_a5a5);
        assert_eq!(cpu.pc, 4);
    }

    #[test]
    fn simd_dup_broadcasts_halfword_lane_zero() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x0e02_07dfu32.to_le_bytes()).unwrap(); // DUP V31.4H,V30.H[0]
        memory.write(4, &0x4e02_07deu32.to_le_bytes()).unwrap(); // DUP V30.8H,V30.H[0]
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[30] = 0xabcd;
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], 0xabcd_abcd_abcd_abcd);
        assert_eq!(cpu.v[30], 0xabcd_abcd_abcd_abcd_abcd_abcd_abcd_abcd);
    }

    #[test]
    fn simd_q_unsigned_store_preserves_general_register_zero() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x3d80_0460u32.to_le_bytes()).unwrap(); // STR Q0,[X3,#16]
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0xfeed_face_cafe_beef);
        cpu.set_x(3, 0x100);
        cpu.v[0] = 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff;
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 0xfeed_face_cafe_beef);
        assert_eq!(cpu.read_vector(0x110, 16).unwrap(), cpu.v[0]);
    }

    #[test]
    fn simd_q_unscaled_store_uses_signed_offset() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x3c9f_0060u32.to_le_bytes()).unwrap(); // STUR Q0,[X3,#-16]
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(3, 0x120);
        cpu.v[0] = u128::MAX;
        cpu.step().unwrap();
        assert_eq!(cpu.read_vector(0x110, 16).unwrap(), u128::MAX);
        assert_eq!(cpu.x(3), 0x120);
    }

    #[test]
    fn simd_s_register_offset_store_scales_index() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xbc23_7820u32.to_le_bytes()).unwrap(); // STR S0,[X1,X3,LSL#2]
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0x100);
        cpu.set_x(3, 3);
        cpu.v[0] = 0xaabb_ccdd;
        cpu.step().unwrap();
        assert_eq!(cpu.read32(0x10c).unwrap(), 0xaabb_ccdd);
    }

    #[test]
    fn simd_uzp1_two_doublewords_handles_overlapping_destination() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x4edf_1bdfu32.to_le_bytes()).unwrap(); // UZP1 V31.2D,V30.2D,V31.2D
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[30] = 0xaaaa_aaaa_aaaa_aaaa_1111_1111_1111_1111;
        cpu.v[31] = 0xbbbb_bbbb_bbbb_bbbb_2222_2222_2222_2222;
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], 0x2222_2222_2222_2222_1111_1111_1111_1111);
    }

    #[test]
    fn simd_ext_eight_rotates_single_vector_by_half() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x6e1f_43ffu32.to_le_bytes()).unwrap(); // EXT V31.16B,V31.16B,V31.16B,#8
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[31] = 0x1122_3344_5566_7788_99aa_bbcc_ddee_ff00;
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], 0x99aa_bbcc_ddee_ff00_1122_3344_5566_7788);
    }

    #[test]
    fn simd_strlen_zero_byte_sequence_builds_general_register_mask() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        for (offset, instruction) in [
            (0, 0x4c40_7020_u32),  // LD1 {V0.16B},[X1]
            (4, 0x4e20_9801_u32),  // CMEQ V1.16B,V0.16B,#0
            (8, 0x0f0c_8422_u32),  // SHRN V2.8B,V1.8H,#4
            (12, 0x9e66_0042_u32), // FMOV X2,D2
        ] {
            memory.write(offset, &instruction.to_le_bytes()).unwrap();
        }
        memory.write(0x100, b"abcdefg\0ijklmnop").unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0x100);
        for _ in 0..4 {
            cpu.step().unwrap();
        }
        assert_ne!(cpu.x(2), 0);
    }

    #[test]
    fn simd_addhn_takes_high_byte_of_wrapping_halfword_sum() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x0e21_4022u32.to_le_bytes()).unwrap(); // ADDHN V2.8B,V1.8H,V1.8H
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[1] = u128::MAX;
        cpu.step().unwrap();
        assert_eq!(cpu.v[2] as u64, u64::MAX);
    }

    #[test]
    fn simd_strchr_compare_and_bit_select_sequence() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        for (offset, instruction) in [
            (0, 0x4f01_e664_u32), // MOVI V4.16B,#0x33
            (4, 0x6e20_8c23_u32), // CMEQ V3.16B,V1.16B,V0.16B
            (8, 0x6ea4_1c62_u32), // BIT V2.16B,V3.16B,V4.16B
        ] {
            memory.write(offset, &instruction.to_le_bytes()).unwrap();
        }
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[0] = u128::from_le_bytes([b'x'; 16]);
        cpu.v[1] = u128::from_le_bytes([b'x'; 16]);
        cpu.v[2] = 0;
        for _ in 0..3 {
            cpu.step().unwrap();
        }
        assert_eq!(cpu.v[4], u128::from_le_bytes([0x33; 16]));
        assert_eq!(cpu.v[3], u128::MAX);
        assert_eq!(cpu.v[2], u128::from_le_bytes([0x33; 16]));
    }

    #[test]
    fn simd_unsigned_compare_and_pairwise_max_sequence() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x6e21_3c62u32.to_le_bytes()).unwrap(); // CMHS V2.16B,V3.16B,V1.16B
        memory.write(4, &0x6e22_a445u32.to_le_bytes()).unwrap(); // UMAXP V5.16B,V2.16B,V2.16B
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[3] = u128::from_le_bytes([2; 16]);
        cpu.v[1] = u128::from_le_bytes([1; 16]);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.v[2], u128::MAX);
        assert_eq!(cpu.v[5], u128::MAX);
    }

    #[test]
    fn simd_pairwise_min_and_eight_byte_zero_compare() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x6e22_ac20u32.to_le_bytes()).unwrap(); // UMINP V0.16B,V1.16B,V2.16B
        memory.write(4, &0x0e20_9800u32.to_le_bytes()).unwrap(); // CMEQ V0.8B,V0.8B,#0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[1] = u128::from_le_bytes([1; 16]);
        cpu.v[2] = u128::from_le_bytes([2; 16]);
        cpu.step().unwrap();
        assert_eq!(&cpu.v[0].to_le_bytes()[..8], &[1; 8]);
        cpu.step().unwrap();
        assert_eq!(cpu.v[0], 0);
    }

    #[test]
    fn fmov_high_lane_preserves_low_lane_and_zero_register() {
        for bits in [0, u64::MAX, 0x7ff0_0000_0000_0001, 0x0123_4567_89ab_cdef] {
            let mut memory = GuestMemory::allocate_on_host(
                GuestPageSize::FOUR_KIB,
                relay_core::HostPageSize::SIXTEEN_KIB,
                4096,
            )
            .unwrap();
            for (i, word) in [0x9eaf_0060u32, 0x9eae_0001, 0x9eaf_03ff, 0x9eae_03ff]
                .into_iter()
                .enumerate()
            {
                memory.write((i * 4) as u64, &word.to_le_bytes()).unwrap();
            }
            let mut cpu = StaticCpu::new(memory, 0).unwrap();
            let prior = 0xfedc_ba98_7654_3210_8877_6655_4433_2211u128;
            cpu.v[0] = prior;
            cpu.v[31] = prior;
            cpu.set_x(3, bits);
            cpu.sysregs.fpsr = 0x0800_009f;
            cpu.step().unwrap();
            let expected = (u128::from(bits) << 64) | u128::from(prior as u64);
            assert_eq!(cpu.v[0], expected);
            #[cfg(all(target_arch = "aarch64", not(miri)))]
            {
                let mut native = 0u128;
                let extracted: u64;
                // Fixed registers mirror the measured instruction, with no FP arithmetic.
                unsafe {
                    core::arch::asm!(
                        "ldr q0, [{input}]", "fmov v0.d[1], x3", "fmov x1, v0.d[1]",
                        "str q0, [{output}]",
                        input = in(reg) &prior, output = in(reg) &mut native,
                        in("x3") bits, out("x1") extracted, out("v0") _,
                        options(nostack, preserves_flags),
                    );
                }
                assert_eq!(native, expected);
                assert_eq!(extracted, bits);
            }
            cpu.step().unwrap();
            assert_eq!(cpu.x(1), bits);
            cpu.step().unwrap();
            assert_eq!(cpu.v[31], u128::from(prior as u64));
            cpu.step().unwrap();
            assert_eq!(cpu.x(31), 0);
            assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
        }
    }

    #[test]
    fn fmov_round_trips_bits_between_general_and_simd_registers() {
        let mut memory = GuestMemory::allocate_on_host(
            GuestPageSize::FOUR_KIB,
            relay_core::HostPageSize::SIXTEEN_KIB,
            4096,
        )
        .unwrap();
        memory.write(0, &0x9e67_001fu32.to_le_bytes()).unwrap(); // FMOV D31,X0
        memory.write(4, &0x9e66_03e1u32.to_le_bytes()).unwrap(); // FMOV X1,D31
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x0123_4567_89ab_cdef);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(1), 0x0123_4567_89ab_cdef);
    }

    #[test]
    fn scalar_fused_guest_instruction_reads_aliased_sources_before_write() {
        let mut memory =
            GuestMemory::allocate_on_host(GuestPageSize::FOUR_KIB, HostPageSize::SIXTEEN_KIB, 4096)
                .unwrap();
        memory.write(0, &0x1f4e_0000u32.to_le_bytes()).unwrap(); // FMADD D0,D0,D14,D0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[0] = u128::from(1.5f64.to_bits()) | (u128::from(u64::MAX) << 64);
        cpu.v[14] = u128::from(2.0f64.to_bits());
        cpu.nzcv = 0xa;
        cpu.sysregs.fpsr = 0x0800_0000;
        cpu.step().unwrap();
        assert_eq!(cpu.v[0], u128::from(4.5f64.to_bits()));
        assert_eq!(cpu.v[14], u128::from(2.0f64.to_bits()));
        assert_eq!(cpu.nzcv, 0xa);
        assert_eq!(cpu.sysregs.fpsr, 0x0800_0000);
        assert_eq!(cpu.pc, 4);
    }

    #[test]
    fn scalar_double_immediate_multiply_and_add_execute() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        for (offset, instruction) in [
            (0, 0x1e6e_101fu32), // FMOV D31,#1.0
            (4, 0x1e7f_0bff),    // FMUL D31,D31,D31
            (8, 0x1e7e_2bff),    // FADD D31,D31,D30
        ] {
            memory.write(offset, &instruction.to_le_bytes()).unwrap();
        }
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[30] = u128::from(2.0f64.to_bits());
        for _ in 0..3 {
            cpu.step().unwrap();
        }
        assert_eq!(cpu.v[31] as u64, 3.0f64.to_bits());
    }

    #[test]
    fn scalar_signed_integer_converts_to_double() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x9e62_0020u32.to_le_bytes()).unwrap(); // SCVTF D0,X1
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, (-42i64) as u64);
        cpu.step().unwrap();
        assert_eq!(cpu.v[0] as u64, (-42.0f64).to_bits());
    }

    #[test]
    fn scalar_double_absolute_clears_only_sign_bit() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x1e60_c01du32.to_le_bytes()).unwrap(); // FABS D29,D0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[0] = u128::from(0xfff8_1234_5678_9abcu64);
        cpu.step().unwrap();
        assert_eq!(cpu.v[29], u128::from(0x7ff8_1234_5678_9abcu64));
    }

    #[test]
    fn scalar_double_move_copies_payload_and_clears_upper_lane() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x1e60_401fu32.to_le_bytes()).unwrap(); // FMOV D31,D0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[0] = (u128::MAX << 64) | u128::from(0x8000_0000_0000_0000u64);
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], u128::from(0x8000_0000_0000_0000u64));
    }

    #[test]
    fn scalar_double_compare_sets_architectural_flags() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x1e7e_23a0u32.to_le_bytes()).unwrap(); // FCMP D29,D30
        memory.write(4, &0x1e60_2008u32.to_le_bytes()).unwrap(); // FCMP D0,#0.0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        for (left, right, expected) in [
            (1.0f64, 2.0f64, 8),
            (2.0f64, 2.0f64, 6),
            (3.0f64, 2.0f64, 2),
            (f64::NAN, 2.0f64, 3),
        ] {
            cpu.pc = 0;
            cpu.v[29] = u128::from(left.to_bits());
            cpu.v[30] = u128::from(right.to_bits());
            cpu.step().unwrap();
            assert_eq!(cpu.nzcv, expected);
        }
        for (value, expected) in [(-1.0f64, 8), (-0.0f64, 6), (1.0f64, 2), (f64::NAN, 3)] {
            cpu.pc = 4;
            cpu.v[0] = u128::from(value.to_bits());
            cpu.step().unwrap();
            assert_eq!(cpu.nzcv, expected);
        }
    }

    #[test]
    fn scalar_double_converts_to_saturated_unsigned_word() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x1e79_0000u32.to_le_bytes()).unwrap(); // FCVTZU W0,D0
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        for (value, expected) in [
            (-1.0f64, 0u64),
            (42.9f64, 42u64),
            (f64::INFINITY, u64::from(u32::MAX)),
            (f64::NAN, 0u64),
        ] {
            cpu.pc = 0;
            cpu.v[0] = u128::from(value.to_bits());
            cpu.step().unwrap();
            assert_eq!(cpu.x(0), expected);
        }
    }

    #[test]
    fn fmov_round_trips_word_bits_between_general_and_simd_registers() {
        let mut memory = GuestMemory::allocate_on_host(
            GuestPageSize::FOUR_KIB,
            relay_core::HostPageSize::SIXTEEN_KIB,
            4096,
        )
        .unwrap();
        memory.write(0, &0x1e27_001fu32.to_le_bytes()).unwrap(); // FMOV S31,W0
        memory.write(4, &0x1e26_03e1u32.to_le_bytes()).unwrap(); // FMOV W1,S31
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0xffff_ffff_89ab_cdef);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.x(1), 0x89ab_cdef);
    }

    #[test]
    fn simd_count_add_and_signed_move_compute_popcount() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        for (offset, instruction) in [
            (0, 0x0e20_5bff_u32), // CNT V31.8B,V31.8B
            (4, 0x0e31_bbff_u32), // ADDV B31,V31.8B
            (8, 0x0e01_2fe0_u32), // SMOV W0,V31.B[0]
        ] {
            memory.write(offset, &instruction.to_le_bytes()).unwrap();
        }
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[31] = 0xf0f0_f0f0_f0f0_f0f0;
        for _ in 0..3 {
            cpu.step().unwrap();
        }
        assert_eq!(cpu.x(0), 32);
    }

    #[test]
    fn simd_string_scan_masks_post_increment_and_pairwise_add() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        for (offset, instruction) in [
            (0, 0x6f07_9604_u32), // BIC V4.8H,#0xf0
            (4, 0x4cdf_7041_u32), // LD1 {V1.16B},[X2],#16
            (8, 0x4e22_bc45_u32), // ADDP V5.16B,V2.16B,V2.16B
        ] {
            memory.write(offset, &instruction.to_le_bytes()).unwrap();
        }
        memory.write(0x100, &[1; 16]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.v[4] = u128::MAX;
        cpu.v[2] = u128::from_le_bytes([1; 16]);
        cpu.set_x(2, 0x100);
        for _ in 0..3 {
            cpu.step().unwrap();
        }
        assert_eq!(cpu.x(2), 0x110);
        assert_eq!(cpu.v[1], u128::from_le_bytes([1; 16]));
        assert_eq!(cpu.v[5], u128::from_le_bytes([2; 16]));
        assert_eq!(cpu.v[4] & 0x00f0, 0);
    }

    #[test]
    fn signed_word_pair_load_sign_extends_both_elements() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x6940_8821u32.to_le_bytes()).unwrap(); // LDPSW X1,X2,[X1,#4]
        memory.write(0x104, &(-2i32).to_le_bytes()).unwrap();
        memory.write(0x108, &3i32.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(1, 0x100);
        cpu.step().unwrap();
        assert_eq!(cpu.x(1), (-2i64) as u64);
        assert_eq!(cpu.x(2), 3);
    }

    #[test]
    fn exclusive_word_update_succeeds_on_single_vcpu() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x885f_7c90u32.to_le_bytes()).unwrap(); // LDXR W16,[X4]
        memory.write(4, &0x8811_7c90u32.to_le_bytes()).unwrap(); // STXR W17,W16,[X4]
        memory.write(0x100, &7u32.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(4, 0x100);
        cpu.step().unwrap();
        assert_eq!(cpu.x(16), 7);
        cpu.set_x(16, 8);
        cpu.step().unwrap();
        assert_eq!(cpu.x(17), 0);
        let mut stored = [0; 4];
        cpu.memory.read(0x100, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 8);
    }

    #[test]
    fn exclusive_byte_update_succeeds_on_single_vcpu() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x085f_7c81u32.to_le_bytes()).unwrap(); // LDXRB W1,[X4]
        memory.write(4, &0x0802_7c81u32.to_le_bytes()).unwrap(); // STXRB W2,W1,[X4]
        memory.write(0x100, &[7]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(4, 0x100);
        cpu.step().unwrap();
        assert_eq!(cpu.x(1), 7);
        cpu.set_x(1, 8);
        cpu.step().unwrap();
        assert_eq!(cpu.x(2), 0);
        let mut stored = [0; 1];
        cpu.memory.read(0x100, &mut stored).unwrap();
        assert_eq!(stored, [8]);
    }

    #[test]
    fn store_release_unlocks_without_exclusive_reservation() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x089f_fc1fu32.to_le_bytes()).unwrap(); // STLRB WZR,[X0]
        memory.write(0x100, &[1]).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(0, 0x100);
        cpu.step().unwrap();
        let mut stored = [1; 1];
        cpu.memory.read(0x100, &mut stored).unwrap();
        assert_eq!(stored, [0]);
        assert_eq!(cpu.exclusive, None);
    }

    #[test]
    fn load_acquire_does_not_create_exclusive_reservation() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0x88df_fc81u32.to_le_bytes()).unwrap(); // LDAR W1,[X4]
        memory.write(0x100, &7u32.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(4, 0x100);
        cpu.step().unwrap();
        assert_eq!(cpu.x(1), 7);
        assert_eq!(cpu.exclusive, None);
    }

    #[test]
    fn pair_exclusive_updates_full_128_bit_value() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0xc87f_04a0u32.to_le_bytes()).unwrap(); // LDXP X0,X1,[X5]
        memory.write(4, &0xc824_8ca2u32.to_le_bytes()).unwrap(); // STLXP W4,X2,X3,[X5]
        memory.write(0x100, &11u64.to_le_bytes()).unwrap();
        memory.write(0x108, &12u64.to_le_bytes()).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.set_x(5, 0x100);
        cpu.step().unwrap();
        assert_eq!(cpu.x(0), 11);
        assert_eq!(cpu.x(1), 12);
        cpu.set_x(2, 21);
        cpu.set_x(3, 22);
        cpu.step().unwrap();
        assert_eq!(cpu.x(4), 0);
        assert_eq!(cpu.read64(0x100).unwrap(), 21);
        assert_eq!(cpu.read64(0x108).unwrap(), 22);
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

#[cfg(test)]
mod native_reference;

#[cfg(test)]
mod el1_reference;

#[cfg(feature = "kernel-probe")]
pub mod kernel_probe;

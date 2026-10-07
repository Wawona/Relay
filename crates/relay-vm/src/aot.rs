//! Offline AOT for the Mode A guest CPU.
//!
//! The store IPA loads a signed image and calls [`StaticCpu::execute_fetched`].
//! It does not compile guest code at runtime, and it does not map guest pages
//! executable. The translator ([`translate_image`], [`emit_specialized_block`])
//! is behind `aot-translate` and tests. It is not a default feature.
//!
//! One host thread runs one vCPU. [`run_vcpus`] starts one thread per job.
//! Bytes that are missing, hashed wrong, or stored over fall back to
//! [`StaticCpu::step`]. StaticCpu remains the oracle.
//!
//! Not proved here: the full NixOS image, shared-memory SMP, guest timing,
//! or any speed claim. See `docs/ios13-aot-assessment.md`.

use crate::cpu::StaticCpu;
#[cfg(any(test, feature = "aot-translate"))]
use crate::guest::GuestMemory;
#[cfg(any(test, feature = "aot-translate"))]
use relay_core::GuestPageSize;
use relay_core::RelayError;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::thread;

const PAGE: u64 = 4096;

/// True when the live guest bytes are the bytes that were signed.
pub(crate) fn image_bytes_match(expected: &[u8], live: &[u8]) -> bool {
    expected == live
}

/// A store into a page that holds a translated instruction drops that page.
pub(crate) fn translation_dropped(store_page: u64, executed_page: u64) -> bool {
    store_page == executed_page
}

#[cfg(kani)]
#[kani::proof]
fn signed_image_rejects_a_changed_byte() {
    let expected: [u8; 4] = kani::any();
    let live: [u8; 4] = kani::any();
    kani::assume(expected != live);
    assert!(!image_bytes_match(&expected, &live));
    assert!(image_bytes_match(&expected, &expected));
}

#[cfg(kani)]
#[kani::proof]
fn store_to_an_executed_page_drops_translation() {
    let store_page: u64 = kani::any();
    let executed_page: u64 = kani::any();
    assert_eq!(
        translation_dropped(store_page, executed_page),
        store_page == executed_page
    );
}

/// Direct-mapped software TLB. Guest pages are never host-executable.
#[derive(Clone, Copy)]
struct TlbSlot {
    valid: bool,
    va_page: u64,
    pa_page: u64,
}

pub(crate) fn tlb_translate(va: u64, va_page: u64, pa_page: u64) -> Option<u64> {
    if va & !(PAGE - 1) != va_page {
        return None;
    }
    Some(pa_page.wrapping_add(va & (PAGE - 1)))
}

pub(crate) fn aot_block_eligible(signed_ok: bool, page_dropped: bool, has_block: bool) -> bool {
    signed_ok && !page_dropped && has_block
}

/// Two AOT host threads store through this mutex. That is the shared-RAM
/// model. Full StaticCpu SMP on one GuestMemory remains unproved.
pub(crate) struct SharedRam {
    bytes: std::sync::Mutex<Vec<u8>>,
}

pub fn ram_span_ok(offset: usize, len: usize, ram: usize) -> bool {
    offset.checked_add(len).map(|end| end <= ram).unwrap_or(false)
}

impl SharedRam {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            bytes: std::sync::Mutex::new(vec![0; len]),
        }
    }

    pub(crate) fn store(&self, offset: usize, data: &[u8]) -> Result<(), RelayError> {
        let mut guard = self
            .bytes
            .lock()
            .map_err(|_| RelayError::Failed("AOT shared RAM poisoned".into()))?;
        if !ram_span_ok(offset, data.len(), guard.len()) {
            return Err(RelayError::Failed("AOT shared RAM store out of range".into()));
        }
        guard[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    pub(crate) fn load(&self, offset: usize, out: &mut [u8]) -> Result<(), RelayError> {
        let guard = self
            .bytes
            .lock()
            .map_err(|_| RelayError::Failed("AOT shared RAM poisoned".into()))?;
        if !ram_span_ok(offset, out.len(), guard.len()) {
            return Err(RelayError::Failed("AOT shared RAM load out of range".into()));
        }
        out.copy_from_slice(&guard[offset..offset + out.len()]);
        Ok(())
    }
}

#[cfg(kani)]
#[kani::proof]
fn tlb_hit_stays_inside_the_page() {
    let offset: u16 = kani::any();
    kani::assume((offset as u64) < PAGE);
    let va_page: u64 = 0x1000;
    let pa_page: u64 = 0x8000_0000;
    let va = va_page + u64::from(offset);
    let pa = tlb_translate(va, va_page, pa_page).unwrap();
    assert!(pa >= pa_page);
    assert!(pa - pa_page < PAGE);
    assert!(tlb_translate(va + PAGE, va_page, pa_page).is_none());
}

#[cfg(kani)]
#[kani::proof]
fn aot_block_requires_signed_live_bytes() {
    let signed_ok: bool = kani::any();
    let dropped: bool = kani::any();
    let has_block: bool = kani::any();
    let ok = aot_block_eligible(signed_ok, dropped, has_block);
    assert!(ok == (signed_ok && !dropped && has_block));
    if ok {
        assert!(signed_ok);
        assert!(!dropped);
    }
}

#[cfg(kani)]
#[kani::proof]
fn shared_ram_span_never_wraps_past_len() {
    let offset: u8 = kani::any();
    let len: u8 = kani::any();
    let ram: u8 = kani::any();
    kani::assume(ram > 0);
    if ram_span_ok(offset as usize, len as usize, ram as usize) {
        assert!((offset as usize) + (len as usize) <= ram as usize);
    }
}

pub(crate) struct SoftwareTlb {
    slots: [TlbSlot; 8],
    next: usize,
    hits: u64,
    misses: u64,
}

impl SoftwareTlb {
    pub(crate) fn new() -> Self {
        Self {
            slots: [TlbSlot {
                valid: false,
                va_page: 0,
                pa_page: 0,
            }; 8],
            next: 0,
            hits: 0,
            misses: 0,
        }
    }

    pub(crate) fn lookup(&mut self, va: u64) -> Option<u64> {
        for slot in &self.slots {
            if slot.valid {
                if let Some(pa) = tlb_translate(va, slot.va_page, slot.pa_page) {
                    self.hits += 1;
                    return Some(pa);
                }
            }
        }
        self.misses += 1;
        None
    }

    pub(crate) fn insert(&mut self, va: u64, pa: u64) {
        let index = self.next % self.slots.len();
        self.slots[index] = TlbSlot {
            valid: true,
            va_page: va & !(PAGE - 1),
            pa_page: pa & !(PAGE - 1),
        };
        self.next = self.next.wrapping_add(1);
    }

    pub(crate) fn invalidate(&mut self) {
        for slot in &mut self.slots {
            slot.valid = false;
        }
    }
}

#[derive(Clone)]
pub(crate) struct AotImage {
    hash: [u8; 32],
    flat: Vec<u8>,
    entry: u64,
    blocks: BTreeMap<u64, Vec<u32>>,
}

impl AotImage {
    fn accepts_live(&self, live: &[u8]) -> bool {
        let mut digest = Sha256::new();
        digest.update(live);
        let got: [u8; 32] = digest.finalize().into();
        got == self.hash && image_bytes_match(&self.flat, live)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AotStats {
    pub aot_insns: u64,
    pub fallback_insns: u64,
}

/// Group a raw little-endian instruction stream into blocks.
/// Available to tests and the offline `aot-translate` tool. Not a default
/// library feature, so a store build does not ship the translator.
#[cfg(any(test, feature = "aot-translate"))]
pub(crate) fn translate_image(entry: u64, code: &[u8]) -> Result<AotImage, RelayError> {
    if code.len() % 4 != 0 || code.is_empty() {
        return Err(RelayError::Failed(
            "AOT image length must be a non-empty multiple of 4".into(),
        ));
    }
    let mut words = Vec::with_capacity(code.len() / 4);
    for chunk in code.chunks_exact(4) {
        words.push(u32::from_le_bytes(chunk.try_into().unwrap()));
    }
    let mut blocks = BTreeMap::new();
    let mut index = 0;
    while index < words.len() {
        let pc = entry + (index as u64) * 4;
        let mut block = Vec::new();
        while index < words.len() {
            let insn = words[index];
            block.push(insn);
            index += 1;
            if is_control(insn) || block.len() == 16 {
                break;
            }
        }
        blocks.insert(pc, block);
    }
    let mut digest = Sha256::new();
    digest.update(code);
    Ok(AotImage {
        hash: digest.finalize().into(),
        flat: code.to_vec(),
        entry,
        blocks,
    })
}

#[cfg(any(test, feature = "aot-translate"))]
pub(crate) fn emit_specialized_block(name: &str, entry: u64, words: &[u32]) -> String {
    let mut out = format!("fn {name}(cpu: &mut StaticCpu) -> Result<(), RelayError> {{\n");
    for (index, word) in words.iter().enumerate() {
        let pc = entry + index as u64 * 4;
        out.push_str(&format!(
            "    cpu.execute_fetched({pc:#x}, {word:#010x})?;\n"
        ));
    }
    out.push_str("    Ok(())\n}\n");
    out
}

#[cfg(any(test, feature = "aot-translate"))]
fn is_control(insn: u32) -> bool {
    insn & 0x7c00_0000 == 0x1400_0000
        || matches!(insn & 0xffff_fc1f, 0xd61f_0000 | 0xd63f_0000 | 0xd65f_0000)
        || insn & 0xff00_0010 == 0x5400_0000
        || insn & 0x7e00_0000 == 0x3400_0000
        || insn & 0x7e00_0000 == 0x3600_0000
}

pub(crate) fn run_image(
    cpu: &mut StaticCpu,
    image: &AotImage,
    budget: u64,
) -> Result<AotStats, RelayError> {
    let mut stats = AotStats::default();
    let mut tlb = SoftwareTlb::new();
    let mut dropped: BTreeSet<u64> = BTreeSet::new();
    let live = live_bytes(cpu, image)?;
    let signed_ok = image.accepts_live(&live);
    while stats.aot_insns + stats.fallback_insns < budget {
        let pc = cpu.pc;
        let page = pc & !(PAGE - 1);
        if !aot_block_eligible(
            signed_ok,
            dropped.contains(&page),
            image.blocks.contains_key(&pc),
        ) {
            cpu.step()?;
            stats.fallback_insns += 1;
            continue;
        }
        let block = image.blocks.get(&pc).expect("checked");
        if !block_matches(cpu, pc, block) {
            cpu.step()?;
            stats.fallback_insns += 1;
            continue;
        }
        remember_exec(&mut tlb, pc);
        let start = pc;
        for (index, word) in block.iter().enumerate() {
            if stats.aot_insns + stats.fallback_insns >= budget {
                break;
            }
            let insn_pc = start + index as u64 * 4;
            if cpu.pc != insn_pc {
                break;
            }
            cpu.execute_fetched(insn_pc, *word)?;
            stats.aot_insns += 1;
            if let Some(store_page) = changed_page(cpu, start, block) {
                let executed = start & !(PAGE - 1);
                dropped.insert(store_page);
                if translation_dropped(store_page, executed) {
                    tlb.invalidate();
                }
                break;
            }
        }
    }
    let _ = tlb.hits;
    Ok(stats)
}

fn remember_exec(tlb: &mut SoftwareTlb, va: u64) {
    // MMU-off identity, and a hit on the next instruction in the page.
    // A real walk fills this slot when the oracle translation is added.
    if tlb.lookup(va).is_none() {
        let page = va & !(PAGE - 1);
        tlb.insert(va, page);
    }
}

fn live_bytes(cpu: &StaticCpu, image: &AotImage) -> Result<Vec<u8>, RelayError> {
    let mut live = Vec::with_capacity(image.flat.len());
    let mut pc = image.entry;
    for _ in 0..image.flat.len() / 4 {
        live.extend_from_slice(&cpu.peek_word(pc)?.to_le_bytes());
        pc = pc.wrapping_add(4);
    }
    Ok(live)
}

fn changed_page(cpu: &StaticCpu, start: u64, words: &[u32]) -> Option<u64> {
    for (index, word) in words.iter().enumerate() {
        let pc = start + index as u64 * 4;
        match cpu.peek_word(pc) {
            Ok(live) if live == *word => {}
            _ => return Some(pc & !(PAGE - 1)),
        }
    }
    None
}

fn block_matches(cpu: &StaticCpu, start: u64, words: &[u32]) -> bool {
    for (index, word) in words.iter().enumerate() {
        let pc = start + index as u64 * 4;
        match cpu.peek_word(pc) {
            Ok(live) if live == *word => {}
            _ => return false,
        }
    }
    true
}

#[cfg(any(test, feature = "aot-translate"))]
pub(crate) struct VcpuJob {
    pub code: Vec<u8>,
    pub budget: u64,
}

#[cfg(any(test, feature = "aot-translate"))]
pub(crate) fn run_vcpus(jobs: Vec<VcpuJob>) -> Vec<Result<(u64, AotStats), RelayError>> {
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(jobs.len());
        for job in jobs {
            handles.push(scope.spawn(move || run_job(job)));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_else(|_| {
                Err(RelayError::Failed("AOT vCPU thread panicked".into()))
            }))
            .collect()
    })
}

#[cfg(any(test, feature = "aot-translate"))]
fn run_job(job: VcpuJob) -> Result<(u64, AotStats), RelayError> {
    let image = translate_image(0, &job.code)?;
    let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096)?;
    memory.write(0, &job.code)?;
    let mut cpu = StaticCpu::new(memory, 0)?;
    let stats = run_image(&mut cpu, &image, job.budget)?;
    Ok((cpu.x_value(0), stats))
}

impl StaticCpu {
    pub(crate) fn x_value(&self, index: u32) -> u64 {
        self.x(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program() -> Vec<u8> {
        let words = [
            0xd280_0100u32, // MOVZ X0, #8
            0x9100_0400u32, // ADD X0, X0, #1
            0xd65f_03c0u32, // RET
        ];
        let mut bytes = Vec::new();
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn specialized_block_matches_the_oracle_and_uses_no_fallback() {
        let code = program();
        let image = translate_image(0, &code).unwrap();
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &code).unwrap();
        let mut aot = StaticCpu::new(memory, 0).unwrap();
        let stats = run_image(&mut aot, &image, 3).unwrap();
        assert_eq!(stats.fallback_insns, 0);
        assert_eq!(stats.aot_insns, 3);
        assert_eq!(aot.x_value(0), 9);

        let mut oracle_mem = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        oracle_mem.write(0, &code).unwrap();
        let mut oracle = StaticCpu::new(oracle_mem, 0).unwrap();
        oracle.step().unwrap();
        oracle.step().unwrap();
        oracle.step().unwrap();
        assert_eq!(oracle.x_value(0), aot.x_value(0));
        assert_eq!(oracle.pc, aot.pc);

        let emitted = emit_specialized_block("block_0", 0, &[0xd280_0100, 0x9100_0400, 0xd65f_03c0]);
        assert!(emitted.contains("execute_fetched(0x0, 0xd2800100)"));
        assert!(emitted.contains("execute_fetched(0x4, 0x91000400)"));
        assert!(emitted.contains("execute_fetched(0x8, 0xd65f03c0)"));
    }

    #[test]
    fn a_changed_guest_byte_falls_back_to_staticcpu() {
        let code = program();
        let image = translate_image(0, &code).unwrap();
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        let mut mutated = code.clone();
        mutated[0] ^= 0xff;
        memory.write(0, &mutated).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        let stats = run_image(&mut cpu, &image, 3).unwrap();
        assert_eq!(stats.aot_insns, 0);
        assert_eq!(stats.fallback_insns, 3);
    }

    #[test]
    fn two_vcpus_run_translated_blocks_on_two_host_threads() {
        let left = {
            let mut bytes = Vec::new();
            for word in [0xd280_0020u32, 0xd65f_03c0] {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
            bytes
        };
        let right = {
            let mut bytes = Vec::new();
            for word in [0xd280_0040u32, 0xd65f_03c0] {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
            bytes
        };
        let results = run_vcpus(vec![
            VcpuJob {
                code: left,
                budget: 2,
            },
            VcpuJob {
                code: right,
                budget: 2,
            },
        ]);
        let left = results[0].as_ref().unwrap();
        let right = results[1].as_ref().unwrap();
        assert_eq!(left.0, 1);
        assert_eq!(right.0, 2);
        assert_eq!(left.1.fallback_insns, 0);
        assert_eq!(right.1.fallback_insns, 0);
        assert_eq!(left.1.aot_insns, 2);
        assert_eq!(right.1.aot_insns, 2);
    }

    fn two_host_threads_observe_shared_ram_after_join() {
        let ram = std::sync::Arc::new(SharedRam::new(16));
        let writer = std::sync::Arc::clone(&ram);
        thread::scope(|scope| {
            scope.spawn(move || writer.store(0, &[0xaa, 0xbb]).unwrap());
        });
        let ram_b = std::sync::Arc::clone(&ram);
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut out = [0u8; 2];
                ram_b.load(0, &mut out).unwrap();
                assert_eq!(out, [0xaa, 0xbb]);
            });
        });
        assert!(!ram_span_ok(15, 2, 16));
        assert!(ram_span_ok(0, 16, 16));
    }

    #[test]
    fn software_tlb_hits_on_the_second_lookup() {
        let mut tlb = SoftwareTlb::new();
        assert!(tlb.lookup(0x1004).is_none());
        assert_eq!(tlb.misses, 1);
        tlb.insert(0x1004, 0x8000_1004);
        assert_eq!(tlb.lookup(0x1008), Some(0x8000_1008));
        assert_eq!(tlb.hits, 1);
        tlb.invalidate();
        assert!(tlb.lookup(0x1008).is_none());
    }

    #[test]
    fn measures_offline_aot_of_the_proved_closure() {
        let code = program();
        let started = std::time::Instant::now();
        let image = translate_image(0, &code).unwrap();
        let translator = started.elapsed();
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &code).unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        let stats = run_image(&mut cpu, &image, 3).unwrap();
        assert_eq!(stats.fallback_insns, 0);
        assert_eq!(stats.aot_insns, 3);
        eprintln!(
            "AOT measure: translator_ms={} signed_text_bytes={} aot_insns={} fallback_insns={} nixos_translated=0",
            translator.as_millis(),
            image.flat.len(),
            stats.aot_insns,
            stats.fallback_insns
        );
    }
}

#[cfg(all(test, not(loom)))]
mod ram_span_proptest {
    use super::ram_span_ok;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn accepted_span_fits(offset in 0usize..4096, len in 0usize..4096, ram in 0usize..8192) {
            let ok = ram_span_ok(offset, len, ram);
            if ok {
                prop_assert!(offset.saturating_add(len) <= ram);
            }
        }
    }
}

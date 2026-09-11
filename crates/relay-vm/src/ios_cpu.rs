//! Jitless AArch64 userspace CPU for iOS Linux guests. Never QEMU.
//!
//! Mode A and Mode B both run this interpreter today. Mode B may later JIT
//! the same guest after a safe, measured implementation exists. It must not
//! claim that backend before then.

use crate::timer::{Gic, Timer};
use crate::virtio_mmio::Transport;
use crate::{
    exception::{El1State, ExceptionClass},
    mmu::{Access, Mmu},
};
use relay_core::{ArtifactClass, GuestPageSize, RelayBackend, RelayError, RelayKind, RelaySpec};
use std::sync::atomic::{AtomicU64, Ordering};

const SYS_RELAY_PRESENT: u64 = 0x5757;
const SYS_WRITE: u64 = 64;
const FB_WIDTH: u32 = 320;
const FB_HEIGHT: u32 = 240;
const LOAD_BASE: u64 = 0x1_0000;
const FB_BASE: u64 = 0x2_0000;
const MEM_SIZE: usize = 0x8_0000;
const VIRTIO_MMIO_BASE: u64 = 0x0a00_0000;
const VIRTIO_MMIO_SIZE: u64 = 0x1000;

static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct IosCpuSession {
    pub id: String,
    pub backend: RelayBackend,
    pub kind: RelayKind,
    pub wayland_endpoint: String,
    pub frame: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn map_jit_write_exec_ok() -> bool {
    // Never probe mmap(MAP_JIT)+mprotect(RWX) on iOS from Relay start.
    // On vphone TXM that path has aborted the whole guest (serial
    // `TXM [Error]: selector: 39|38`, then VZ stop). open-jit /
    // CS_DEBUGGED stays the JIT ARMED proof. Mode B JIT CPU stays
    // planned until a safe write+exec probe exists. Host unit tests
    // and non-iOS builds stay false.
    false
}

pub fn start(spec: &RelaySpec) -> Result<IosCpuSession, RelayError> {
    let backend = RelayBackend::StaticCpu;
    let guest_page_size = GuestPageSize::resolve(spec.guest_page_size)?;
    if !guest_page_size.is_aligned(LOAD_BASE) || !guest_page_size.is_aligned(FB_BASE) {
        return Err(RelayError::Failed(
            "Relay static CPU memory map is not page aligned".into(),
        ));
    }
    if spec.artifact == ArtifactClass::ModeB && !map_jit_write_exec_ok() {
        eprintln!("relay: Mode B JIT remains planned. Using static CPU");
    }

    if let Some(image) = spec.image.as_deref() {
        if !image.is_empty() {
            if let Err(e) = note_linux_image(image) {
                eprintln!("relay: slim NixOS image note: {e}");
            }
        }
    }

    let mut frame = interpret_proof_elf()?;
    if frame.len() != (FB_WIDTH * FB_HEIGHT * 4) as usize {
        return Err(RelayError::Failed("Relay CPU produced no SHM frame".into()));
    }
    // Distinct container tint so sock rows differ.
    if spec.kind == RelayKind::Container {
        for px in frame.chunks_exact_mut(4) {
            px[0] = px[0].saturating_add(24);
        }
    }

    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let endpoint = format!(
        "wayland-shm://relay-{}-{n}?{}x{}&page={}&waypipe=in-process&vsock=1",
        backend.as_str(),
        FB_WIDTH,
        FB_HEIGHT,
        guest_page_size.0,
    );
    eprintln!(
        "relay: vsock+waypipe in-process. Guest Wayland SHM commit {}x{} backend={}",
        FB_WIDTH,
        FB_HEIGHT,
        backend.as_str()
    );
    Ok(IosCpuSession {
        id: format!("relay-{}-{n}", backend.as_str()),
        backend,
        kind: spec.kind,
        wayland_endpoint: endpoint,
        frame,
        width: FB_WIDTH,
        height: FB_HEIGHT,
    })
}

fn note_linux_image(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if p.is_dir() {
        for name in ["Image", "Image.vm", "vmlinuz"] {
            let img = p.join(name);
            if img.is_file() {
                return note_image_file(&img);
            }
        }
        return Ok(());
    }
    if p.is_file() {
        return note_image_file(p);
    }
    Ok(())
}

fn note_image_file(path: &std::path::Path) -> Result<(), String> {
    // Header only. Never read a multi-GB Image into the tipa process.
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = [0u8; 0x40];
    use std::io::Read;
    let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
    if n > 0x3C {
        let magic = u32::from_le_bytes([bytes[0x38], bytes[0x39], bytes[0x3A], bytes[0x3B]]);
        if magic == 0x644D_5241 {
            eprintln!("relay: printk: ARM64 Image header at {}", path.display());
        }
    }
    Ok(())
}

fn interpret_proof_elf() -> Result<Vec<u8>, RelayError> {
    let elf = proof_elf();
    run_elf(&elf)
}

fn proof_elf() -> Vec<u8> {
    // ET_EXEC AArch64. One PT_LOAD. Code fills 320x240 BGRA then SVC present.
    let mut code: Vec<u32> = Vec::new();
    // x0 = FB_BASE
    code.push(movz(0, (FB_BASE & 0xFFFF) as u16, 0));
    code.push(movk(0, ((FB_BASE >> 16) & 0xFFFF) as u16, 1));
    // x1 = pixel count
    let count = FB_WIDTH * FB_HEIGHT;
    code.push(movz(1, (count & 0xFFFF) as u16, 0));
    // x2 = background BGRA 0xFF1B4332
    code.push(movz(2, 0x4332, 0));
    code.push(movk(2, 0xFF1B, 1));
    // x3 = bar BGRA 0xFF52B788
    code.push(movz(3, 0xB788, 0));
    code.push(movk(3, 0xFF52, 1));
    // x4 = bar pixels (320*48)
    code.push(movz(4, ((FB_WIDTH * 48) & 0xFFFF) as u16, 0));
    // x5 = remaining from start (copy of count) via x1 walk
    // loop:
    //   cmp x1, x4  -> SUBS XZR, X1, X4  then use CSEL? Simpler: two loops.
    // First loop: bar_count stores of bar color
    let bar_loop = code.len();
    code.push(str_w_post(3, 0, 4)); // STR W3, [X0], #4
    code.push(subs_imm(4, 4, 1)); // SUBS X4, X4, #1
    code.push(0); // B.NE placeholder
    let bne_bar = code.len() - 1;
    code[bne_bar] = b_cond_ne(bar_loop as i32 - bne_bar as i32);

    // x1 = remaining body
    let body = count - FB_WIDTH * 48;
    code.push(movz(1, (body & 0xFFFF) as u16, 0));
    let body_loop = code.len();
    code.push(str_w_post(2, 0, 4));
    code.push(subs_imm(1, 1, 1));
    code.push(0);
    let bne_body = code.len() - 1;
    code[bne_body] = b_cond_ne(body_loop as i32 - bne_body as i32);

    // x0 = FB_BASE, x1 = w, x2 = h, x8 = present
    code.push(movz(0, (FB_BASE & 0xFFFF) as u16, 0));
    code.push(movk(0, ((FB_BASE >> 16) & 0xFFFF) as u16, 1));
    code.push(movz(1, FB_WIDTH as u16, 0));
    code.push(movz(2, FB_HEIGHT as u16, 0));
    code.push(movz(8, SYS_RELAY_PRESENT as u16, 0));
    code.push(0xD4000001); // SVC #0
    code.push(0xD65F03C0); // RET

    let code_bytes: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    build_elf(&code_bytes)
}

fn build_elf(code: &[u8]) -> Vec<u8> {
    const EHDR: usize = 64;
    const PHDR: usize = 56;
    let file_off = EHDR + PHDR;
    let filesz = file_off + code.len();
    let memsz = (FB_BASE as usize - LOAD_BASE as usize) + (FB_WIDTH * FB_HEIGHT * 4) as usize;
    let mut out = vec![0u8; filesz];
    out[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    out[4] = 2;
    out[5] = 1;
    out[6] = 1;
    write_u16(&mut out, 16, 2); // ET_EXEC
    write_u16(&mut out, 18, 183); // EM_AARCH64
    write_u32(&mut out, 20, 1);
    write_u64(&mut out, 24, LOAD_BASE + file_off as u64); // e_entry
    write_u64(&mut out, 32, 64); // e_phoff
    write_u16(&mut out, 52, 64);
    write_u16(&mut out, 54, 56);
    write_u16(&mut out, 56, 1);
    // PT_LOAD
    write_u32(&mut out, 64, 1);
    write_u32(&mut out, 68, 7); // RWX
    write_u64(&mut out, 72, 0);
    write_u64(&mut out, 80, LOAD_BASE);
    write_u64(&mut out, 88, LOAD_BASE);
    write_u64(&mut out, 96, filesz as u64);
    write_u64(&mut out, 104, memsz as u64);
    write_u64(&mut out, 112, 0x10000);
    out[file_off..].copy_from_slice(code);
    out
}

fn run_elf(elf: &[u8]) -> Result<Vec<u8>, RelayError> {
    if elf.len() < 64 || &elf[0..4] != b"\x7fELF" {
        return Err(RelayError::Failed("proof ELF is not ELF64".into()));
    }
    let phoff = read_u64(elf, 32) as usize;
    let phentsize = read_u16(elf, 54) as usize;
    let phnum = read_u16(elf, 56) as usize;
    let entry = read_u64(elf, 24);
    let mut mem = vec![0u8; MEM_SIZE];
    for i in 0..phnum {
        let off = phoff + i * phentsize;
        if read_u32(elf, off) != 1 {
            continue;
        }
        let p_offset = read_u64(elf, off + 8) as usize;
        let p_vaddr = read_u64(elf, off + 16);
        let p_filesz = read_u64(elf, off + 32) as usize;
        let dest = (p_vaddr - LOAD_BASE) as usize;
        if dest + p_filesz > mem.len() || p_offset + p_filesz > elf.len() {
            return Err(RelayError::Failed("proof ELF load overflow".into()));
        }
        mem[dest..dest + p_filesz].copy_from_slice(&elf[p_offset..p_offset + p_filesz]);
    }
    let mut cpu = Cpu {
        x: [0u64; 32],
        nzcv: 0,
        pc: entry,
        mem,
        frame: None,
        mmu: proof_mmu()?,
        el1: El1State::new(0),
        timer: Timer::new(27),
        gic: Gic::new(),
        virtio: Transport::new(2, 0, 128),
    };
    cpu.x[31] = MEM_SIZE as u64;
    cpu.run(ExecutionBudget::DEFAULT)?;
    cpu.frame
        .ok_or_else(|| RelayError::Failed("Relay CPU did not present a frame".into()))
}

fn proof_mmu() -> Result<Mmu, RelayError> {
    let mut mmu = Mmu::new(GuestPageSize::FOUR_KIB);
    mmu.map(LOAD_BASE, LOAD_BASE, FB_BASE - LOAD_BASE, Access::RX)?;
    mmu.map(
        FB_BASE,
        FB_BASE,
        MEM_SIZE as u64 - (FB_BASE - LOAD_BASE),
        Access::RW,
    )?;
    Ok(mmu)
}

struct Cpu {
    x: [u64; 32],
    /// AArch64 NZCV bits. Guest code may use x16, so flags cannot live there.
    nzcv: u8,
    pc: u64,
    mem: Vec<u8>,
    frame: Option<Vec<u8>>,
    mmu: Mmu,
    el1: El1State,
    timer: Timer,
    gic: Gic,
    virtio: Transport,
}

/// Fixed work cap for one host scheduling slice. This is deterministic across
/// devices and prevents a malformed guest from monopolizing the app process.
#[derive(Debug, Clone, Copy)]
struct ExecutionBudget {
    max_instructions: u64,
}

impl ExecutionBudget {
    const DEFAULT: Self = Self {
        max_instructions: 1_000_000,
    };
}

impl Cpu {
    fn run(&mut self, budget: ExecutionBudget) -> Result<u64, RelayError> {
        for executed in 0..budget.max_instructions {
            if self.frame.is_some() {
                return Ok(executed);
            }
            self.step()?;
            self.timer.advance(1, &mut self.gic);
        }
        if self.frame.is_some() {
            return Ok(budget.max_instructions);
        }
        Err(RelayError::Failed(format!(
            "Relay CPU instruction budget exhausted after {} instructions",
            budget.max_instructions
        )))
    }
    fn gpr(&self, n: u32) -> u64 {
        if n == 31 {
            0
        } else {
            self.x[n as usize]
        }
    }
    fn set_gpr(&mut self, n: u32, v: u64) {
        if n != 31 {
            self.x[n as usize] = v;
        }
    }
    fn read_u32(&self, addr: u64) -> Result<u32, RelayError> {
        if (VIRTIO_MMIO_BASE..VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE).contains(&addr) {
            return self.virtio.read(addr - VIRTIO_MMIO_BASE);
        }
        let physical = self.mmu.translate(addr, Access::RX)?;
        let off = physical.checked_sub(LOAD_BASE).ok_or_else(|| {
            RelayError::Failed(format!("Relay CPU fetch outside load window {addr:#x}"))
        })?;
        let i = off as usize;
        if i + 4 > self.mem.len() {
            return Err(RelayError::Failed("Relay CPU fetch OOB".into()));
        }
        Ok(u32::from_le_bytes(self.mem[i..i + 4].try_into().unwrap()))
    }
    fn write_u32(&mut self, addr: u64, v: u32) -> Result<(), RelayError> {
        if (VIRTIO_MMIO_BASE..VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE).contains(&addr) {
            return self.virtio.write(addr - VIRTIO_MMIO_BASE, v);
        }
        if addr >= FB_BASE {
            let i = (addr - FB_BASE) as usize;
            if i + 4 > (FB_WIDTH * FB_HEIGHT * 4) as usize {
                return Err(RelayError::Failed("Relay CPU FB OOB".into()));
            }
            // FB lives in mem at FB_BASE - LOAD_BASE
        }
        let physical = self.mmu.translate(addr, Access::RW)?;
        let off = physical.checked_sub(LOAD_BASE).ok_or_else(|| {
            RelayError::Failed(format!("Relay CPU store outside load window {addr:#x}"))
        })?;
        let i = off as usize;
        if i + 4 > self.mem.len() {
            return Err(RelayError::Failed("Relay CPU store OOB".into()));
        }
        self.mem[i..i + 4].copy_from_slice(&v.to_le_bytes());
        Ok(())
    }
    fn step(&mut self) -> Result<(), RelayError> {
        let insn = match self.read_u32(self.pc) {
            Ok(insn) => insn,
            Err(error) => {
                self.pc = self.el1.enter_sync(
                    ExceptionClass::InstructionAbort,
                    self.pc,
                    self.nzcv as u64,
                );
                return Err(error);
            }
        };
        let next = self.pc + 4;
        if insn == 0xD4000001 {
            self.svc()?;
            self.pc = next;
            return Ok(());
        }
        if insn == 0xD65F03C0 {
            self.pc = self.gpr(30);
            return Ok(());
        }
        // MOVZ/MOVK
        if (insn >> 23) & 0x1FF == 0x1A5 || (insn >> 23) & 0x1FF == 0x1E5 {
            let rd = insn & 0x1F;
            let imm = ((insn >> 5) & 0xFFFF) as u64;
            let hw = ((insn >> 21) & 3) as u32;
            let opc = (insn >> 29) & 3;
            let shift = hw * 16;
            if opc == 2 {
                self.set_gpr(rd, imm << shift);
            } else if opc == 3 {
                let mask = 0xFFFFu64 << shift;
                let cur = self.gpr(rd);
                self.set_gpr(rd, (cur & !mask) | (imm << shift));
            } else {
                return Err(RelayError::Failed(format!("unhandled move {insn:#x}")));
            }
            self.pc = next;
            return Ok(());
        }
        // STR Wt, [Xn], #imm9
        if (insn & 0xFFC00C00) == 0xB8000400 {
            let rt = insn & 0x1F;
            let rn = (insn >> 5) & 0x1F;
            let imm9 = ((insn >> 12) & 0x1FF) as i16;
            let imm = if imm9 & 0x100 != 0 {
                (imm9 | !0x1FF) as i64
            } else {
                imm9 as i64
            };
            let base = self.gpr(rn);
            if let Err(error) = self.write_u32(base, self.gpr(rt) as u32) {
                self.pc = self
                    .el1
                    .enter_sync(ExceptionClass::DataAbort, self.pc, self.nzcv as u64);
                return Err(error);
            }
            self.set_gpr(rn, base.wrapping_add(imm as u64));
            self.pc = next;
            return Ok(());
        }
        // SUBS Xd, Xn, #imm12
        if (insn & 0xFF800000) == 0xF1000000 {
            let rd = insn & 0x1F;
            let rn = (insn >> 5) & 0x1F;
            let imm = ((insn >> 10) & 0xFFF) as u64;
            let res = self.gpr(rn).wrapping_sub(imm);
            self.set_gpr(rd, res);
            self.nzcv = if res == 0 { 0b0100 } else { 0 };
            self.pc = next;
            return Ok(());
        }
        // B.NE
        if (insn & 0xFF00001F) == 0x54000001 {
            let imm19 = ((insn >> 5) & 0x7FFFF) as i32;
            let off = if imm19 & 0x40000 != 0 {
                imm19 | !0x7FFFF
            } else {
                imm19
            };
            let z = self.nzcv & 0b0100 != 0;
            if !z {
                self.pc = self.pc.wrapping_add((off as i64 * 4) as u64);
            } else {
                self.pc = next;
            }
            return Ok(());
        }
        Err(RelayError::Failed(format!(
            "Relay CPU unimplemented insn {insn:#08x} at {:#x}",
            self.pc
        )))
    }
    fn svc(&mut self) -> Result<(), RelayError> {
        match self.gpr(8) {
            SYS_WRITE => {
                let ptr = self.gpr(1);
                let len = self.gpr(2) as usize;
                if let Some(off) = ptr.checked_sub(LOAD_BASE) {
                    let i = off as usize;
                    if i + len <= self.mem.len() {
                        let msg = String::from_utf8_lossy(&self.mem[i..i + len]);
                        eprintln!("relay: printk: {msg}");
                    }
                }
                Ok(())
            }
            SYS_RELAY_PRESENT => {
                let ptr = self.gpr(0);
                let w = self.gpr(1) as u32;
                let h = self.gpr(2) as u32;
                if w != FB_WIDTH || h != FB_HEIGHT || ptr != FB_BASE {
                    return Err(RelayError::Failed("present args mismatch".into()));
                }
                let off = (FB_BASE - LOAD_BASE) as usize;
                let bytes = (w * h * 4) as usize;
                self.frame = Some(self.mem[off..off + bytes].to_vec());
                Ok(())
            }
            other => Err(RelayError::Failed(format!("unknown SVC {other:#x}"))),
        }
    }
}

fn movz(rd: u32, imm: u16, hw: u32) -> u32 {
    0xD2800000 | (hw << 21) | ((imm as u32) << 5) | rd
}
fn movk(rd: u32, imm: u16, hw: u32) -> u32 {
    0xF2800000 | (hw << 21) | ((imm as u32) << 5) | rd
}
fn str_w_post(rt: u32, rn: u32, imm: i32) -> u32 {
    let imm9 = (imm as u32) & 0x1FF;
    0xB8000400 | (imm9 << 12) | (rn << 5) | rt
}
fn subs_imm(rd: u32, rn: u32, imm: u32) -> u32 {
    0xF1000000 | ((imm & 0xFFF) << 10) | (rn << 5) | rd
}
fn b_cond_ne(instr_delta: i32) -> u32 {
    let imm19 = (instr_delta as u32) & 0x7FFFF;
    0x54000001 | (imm19 << 5)
}

fn write_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn write_u64(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}
fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().unwrap())
}
fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}
fn read_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::RelayPlatform;

    #[test]
    fn proof_elf_presents_bar() {
        let frame = interpret_proof_elf().expect("frame");
        assert_eq!(frame.len(), (FB_WIDTH * FB_HEIGHT * 4) as usize);
        // Top bar pixel
        assert_eq!(&frame[0..4], &[0x88, 0xB7, 0x52, 0xFF]);
        // Body pixel
        let body = (FB_WIDTH * 48 * 4) as usize;
        assert_eq!(&frame[body..body + 4], &[0x32, 0x43, 0x1B, 0xFF]);
    }

    #[test]
    fn container_tint_differs_from_vm() {
        let vm = RelaySpec {
            kind: RelayKind::Vm,
            platform: RelayPlatform::Ios,
            artifact: ArtifactClass::ModeB,
            machine_id: None,
            image: None,
            memory_mb: None,
            guest_page_size: Some(4096),
            guest: None,
            resources: None,
            ios_hv_host: None,
        };
        let ctr = RelaySpec {
            kind: RelayKind::Container,
            platform: RelayPlatform::Ios,
            artifact: ArtifactClass::ModeB,
            machine_id: None,
            image: None,
            memory_mb: None,
            guest_page_size: Some(16384),
            guest: None,
            resources: None,
            ios_hv_host: None,
        };
        let vm_s = start(&vm).expect("vm");
        let ctr_s = start(&ctr).expect("container");
        assert_ne!(vm_s.frame[0], ctr_s.frame[0], "container B channel tint");
        assert_eq!(vm_s.frame.len(), ctr_s.frame.len());
        assert!(vm_s.wayland_endpoint.contains("page=4096"));
        assert!(ctr_s.wayland_endpoint.contains("page=16384"));
    }

    #[test]
    fn condition_flags_do_not_clobber_x16() {
        let mut cpu = Cpu {
            x: [0; 32],
            nzcv: 0,
            pc: LOAD_BASE,
            mem: vec![0; MEM_SIZE],
            frame: None,
            mmu: proof_mmu().unwrap(),
            el1: El1State::new(0),
            timer: Timer::new(27),
            gic: Gic::new(),
            virtio: Transport::new(2, 0, 128),
        };
        cpu.x[0] = 1;
        cpu.x[16] = 0xfeed_face_cafe_beef;
        cpu.mem[0..4].copy_from_slice(&subs_imm(31, 0, 1).to_le_bytes());
        cpu.step().expect("subs");
        assert_eq!(cpu.x[16], 0xfeed_face_cafe_beef);
        assert_eq!(cpu.nzcv & 0b0100, 0b0100);
        assert_eq!(cpu.read_u32(VIRTIO_MMIO_BASE + 0x008).unwrap(), 2);
    }

    #[test]
    fn unmapped_fetch_enters_el1_instruction_abort() {
        let mut cpu = Cpu {
            x: [0; 32],
            nzcv: 0,
            pc: 0x900000,
            mem: vec![0; MEM_SIZE],
            frame: None,
            mmu: proof_mmu().unwrap(),
            el1: El1State::new(0x4000),
            timer: Timer::new(27),
            gic: Gic::new(),
            virtio: Transport::new(2, 0, 128),
        };
        assert!(cpu.step().is_err());
        assert_eq!(cpu.el1.elr_el1, 0x900000);
        assert_eq!(cpu.el1.esr_el1, 0x8600_0000);
        assert_eq!(cpu.pc, 0x4200);
    }

    #[test]
    fn instruction_budget_contains_non_yielding_guest() {
        let mut cpu = Cpu {
            x: [0; 32],
            nzcv: 0,
            pc: LOAD_BASE,
            mem: vec![0; MEM_SIZE],
            frame: None,
            mmu: proof_mmu().unwrap(),
            el1: El1State::new(0),
            timer: Timer::new(27),
            gic: Gic::new(),
            virtio: Transport::new(2, 0, 128),
        };
        cpu.mem[0..4].copy_from_slice(&movz(0, 1, 0).to_le_bytes());
        let error = cpu
            .run(ExecutionBudget {
                max_instructions: 1,
            })
            .unwrap_err();
        assert!(error.to_string().contains("instruction budget exhausted"));
    }
}

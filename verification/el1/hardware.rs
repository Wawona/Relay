//! macOS development oracle only. Never linked into a Relay product.
//! Executes only the fixed micro-guests in cases.rs, with no file/code input.
mod cases;
use std::{
    alloc::{alloc_zeroed, dealloc, Layout},
    ffi::c_void,
    ptr,
};
#[repr(C)]
struct Exit {
    reason: u32,
    syndrome: u64,
    virtual_address: u64,
    physical_address: u64,
}
#[link(name = "Hypervisor", kind = "framework")]
extern "C" {
    fn hv_vm_create(config: *const c_void) -> i32;
    fn hv_vm_map(address: *mut c_void, ipa: u64, size: usize, flags: u64) -> i32;
    fn hv_vcpu_create(cpu: *mut u64, exit: *mut *mut Exit, config: *const c_void) -> i32;
    fn hv_vcpu_set_reg(cpu: u64, reg: u32, value: u64) -> i32;
    fn hv_vcpu_get_reg(cpu: u64, reg: u32, value: *mut u64) -> i32;
    fn hv_vcpu_set_sys_reg(cpu: u64, reg: u16, value: u64) -> i32;
    fn hv_vcpu_get_sys_reg(cpu: u64, reg: u16, value: *mut u64) -> i32;
    fn hv_vcpu_run(cpu: u64) -> i32;
    fn hv_vcpu_set_pending_interrupt(cpu: u64, kind: u32, pending: bool) -> i32;
    fn hv_vcpu_destroy(cpu: u64) -> i32;
    fn hv_vm_destroy() -> i32;
}
fn check(operation: &str, status: i32) {
    assert_eq!(status, 0, "{operation}: {status:#x}");
}
fn main() {
    for page in [4096u32, 16384] {
        for case in cases::cases() {
            // SAFETY: allocation has HV-required 16 KiB alignment/lifetime;
            // only one vCPU runs synchronously. Its exit pointer is read only
            // while the vCPU exists. Mapping is destroyed before freeing RAM.
            unsafe {
                // Fresh VM per case: changing table bytes/config without TLBI
                // would reuse stale hardware translations and spoil the oracle.
                check("VM create", hv_vm_create(ptr::null()));
                let layout = Layout::from_size_align(cases::RAM_BYTES, 16384).unwrap();
                let ram = alloc_zeroed(layout);
                assert!(!ram.is_null());
                let initial = case.memory(page);
                ptr::copy_nonoverlapping(initial.as_ptr(), ram, initial.len());
                check("VM map", hv_vm_map(ram.cast(), 0, cases::RAM_BYTES, 7));
                let mut cpu = 0;
                let mut exit = ptr::null_mut();
                check(
                    "vCPU create",
                    hv_vcpu_create(&mut cpu, &mut exit, ptr::null()),
                );
                assert!(!exit.is_null());
                for (reg, value) in [
                    (31, case.pc()),
                    (34, case.pstate()),
                    (0, 0x1234),
                    (1, case.target()),
                ] {
                    check("set register", hv_vcpu_set_reg(cpu, reg, value));
                }
                for (reg, value) in [
                    (0xc100, 0),
                    (0xc102, case.tcr(page)),
                    (0xc510, 0xff),
                    (0xc600, cases::VECTOR),
                    (0xc080, case.sctlr()),
                    (0xc200, case.saved_pstate()),
                    (0xc201, case.return_pc()),
                    (0xc290, 0),
                    (0xc300, 0),
                    (0xc208, 0x36000),
                    (0xe208, 0x38000),
                ] {
                    check("set system register", hv_vcpu_set_sys_reg(cpu, reg, value));
                }
                if matches!(case.access, cases::Access::Irq) {
                    check("inject IRQ", hv_vcpu_set_pending_interrupt(cpu, 0, true));
                }
                if matches!(case.access, cases::Access::MaskedTimer) {
                    check("timer CVAL", hv_vcpu_set_sys_reg(cpu, 0xdf1a, 0));
                    check("timer CTL", hv_vcpu_set_sys_reg(cpu, 0xdf19, 3));
                }
                check("vCPU run", hv_vcpu_run(cpu));
                assert_eq!((*exit).reason, 1, "{}: unexpected exit", case.name);
                assert_eq!(
                    (*exit).syndrome,
                    0x5a000000,
                    "{}: expected HVC checkpoint",
                    case.name
                );
                let mut pc = 0;
                let mut x0 = 0;
                let mut pstate = 0;
                check("get PC", hv_vcpu_get_reg(cpu, 31, &mut pc));
                check("get X0", hv_vcpu_get_reg(cpu, 0, &mut x0));
                check("get PSTATE", hv_vcpu_get_reg(cpu, 34, &mut pstate));
                // HVC exits after advancing PC. StaticCpu stops before HVC;
                // this explicit checkpoint normalization is the only PC offset.
                pc = pc.checked_sub(4).unwrap();
                let mut state = [0u64; 6];
                for (index, reg) in [0xc200, 0xc201, 0xc290, 0xc300, 0xc208, 0xe208]
                    .into_iter()
                    .enumerate()
                {
                    check(
                        "get system register",
                        hv_vcpu_get_sys_reg(cpu, reg, &mut state[index]),
                    );
                }
                let target = ptr::read(ram.add(cases::TARGET as usize).cast::<u64>());
                let sp = if pstate & 1 == 0 { state[4] } else { state[5] };
                println!("{{\"case\":\"{}\",\"page\":{page},\"pc\":{pc},\"x0\":{x0},\"pstate\":{pstate},\"spsr\":{},\"elr\":{},\"esr\":{},\"far\":{},\"sp0\":{},\"sp1\":{},\"sp\":{sp},\"target\":{target}}}", case.name, state[0], state[1], state[2], state[3], state[4], state[5]);
                check("vCPU destroy", hv_vcpu_destroy(cpu));
                check("VM destroy", hv_vm_destroy());
                dealloc(ram, layout);
            }
        }
    }
}

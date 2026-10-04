//! Compares production interpretation with independently recorded EL1 hardware.
use super::*;
#[path = "../../../../verification/el1/cases.rs"]
mod cases;
#[derive(Debug, serde::Deserialize, PartialEq, Eq)]
struct Observation {
    case: String,
    page: u32,
    pc: u64,
    x0: u64,
    pstate: u64,
    sp0: u64,
    sp1: u64,
    sp: u64,
    spsr: u64,
    elr: u64,
    esr: u64,
    far: u64,
    target: u64,
}
#[test]
fn hardware_el1_checkpoints_match_recorded_reference() {
    let observations: Vec<Observation> =
        include_str!("../../../../verification/el1/reference.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    let inputs: Vec<_> = [4096, 16384]
        .into_iter()
        .flat_map(|page| cases::cases().into_iter().map(move |case| (page, case)))
        .collect();
    assert_eq!(observations.len(), inputs.len());
    let mut mismatches = Vec::new();
    for (expected, (page, case)) in observations.iter().zip(inputs) {
        assert_eq!((expected.case.as_str(), expected.page), (case.name, page));
        let mut memory = GuestMemory::allocate_on_host(
            relay_core::GuestPageSize(page),
            relay_core::HostPageSize::SIXTEEN_KIB,
            cases::RAM_BYTES as u64,
        )
        .unwrap();
        memory.write(0, &case.memory(page)).unwrap();
        let mut cpu = StaticCpu::new(memory, case.pc()).unwrap();
        cpu.sysregs.current_el = case.el;
        cpu.sysregs.spsel = if case.sp0 { 0 } else { 1 };
        cpu.sysregs.daif = case.pstate() & 0x3c0;
        cpu.sysregs.sctlr_el1 = case.sctlr();
        cpu.sysregs.tcr_el1 = case.tcr(page);
        cpu.sysregs.mair_el1 = 0xff;
        cpu.sysregs.vbar_el1 = cases::VECTOR;
        cpu.sysregs.spsr_el1 = case.saved_pstate();
        cpu.sysregs.elr_el1 = case.return_pc();
        cpu.sysregs.sp_el0 = 0x36000;
        cpu.sp_el1 = 0x38000;
        cpu.sp = if case.el == 0 || case.sp0 {
            0x36000
        } else {
            0x38000
        };
        cpu.set_x(0, 0x1234);
        cpu.set_x(1, case.target());
        if matches!(case.access, cases::Access::Irq) {
            cpu.bus.raise_irq(34);
            cpu.take_pending_irq().unwrap();
        }
        if matches!(case.access, cases::Access::MaskedTimer) {
            cpu.sysregs.write(crate::sysregs::CNTV_CVAL_EL0, 0).unwrap();
            cpu.sysregs.write(crate::sysregs::CNTV_CTL_EL0, 3).unwrap();
        }
        let mut reached = false;
        for _ in 0..16 {
            if [
                cases::VECTOR,
                cases::VECTOR + 0x80,
                cases::VECTOR + 0x200,
                cases::VECTOR + 0x280,
                cases::VECTOR + 0x400,
                cases::VECTOR + 0x480,
            ]
            .contains(&cpu.pc)
                || (cpu.pc == case.target() + 4 && !matches!(case.access, cases::Access::Eret(0)))
                || (case.el == 1 && cpu.pc == cases::CODE + 4)
            {
                reached = true;
                break;
            }
            cpu.step()
                .unwrap_or_else(|error| panic!("{} page={page}: {error}", case.name));
        }
        assert!(reached, "{} page={page}: checkpoint not reached", case.name);
        let mut target = [0; 8];
        cpu.memory.read(cases::TARGET, &mut target).unwrap();
        let actual = Observation {
            case: case.name.into(),
            page,
            pc: cpu.pc,
            x0: cpu.x(0),
            pstate: cpu.sysregs.daif
                | (u64::from(cpu.nzcv) << 28)
                | (u64::from(cpu.sysregs.current_el) << 2)
                | cpu.sysregs.spsel,
            sp0: cpu.sysregs.sp_el0,
            sp1: if cpu.sysregs.current_el == 1 && cpu.sysregs.spsel == 1 {
                cpu.sp
            } else {
                cpu.sp_el1
            },
            sp: cpu.sp,
            spsr: cpu.sysregs.spsr_el1,
            elr: cpu.sysregs.elr_el1,
            esr: cpu.sysregs.esr_el1,
            far: cpu.sysregs.far_el1,
            target: u64::from_le_bytes(target),
        };
        if &actual != expected {
            mismatches.push(format!(
                "{} page={page}: actual={actual:?}, hardware={expected:?}",
                case.name
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

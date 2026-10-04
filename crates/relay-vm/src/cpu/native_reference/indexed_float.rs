//! Indexed FMUL comparisons use fixed native instructions, never guest bytes.
use super::*;

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn indexed_fmul_matches_native_all_single_double_forms() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {{
            let mut cpu = cpu_for($word);
            let samples = [0u128, u128::MAX,
                0x7ff0_0000_0000_0001_7f80_0000_0000_0000,
                0x0000_0000_0000_0001_0080_0000_007f_ffff,
                0x8000_0000_0000_0000_8000_0000_8000_0000,
                0x3ff0_0000_0000_0000_3f80_0000_3f00_0000];
            for left in samples { for right in samples { for control in 0..16u64 {
                let fpcr = control << 22;
                let mut native = 0u128;
                let status: u64;
                // SAFETY: fixed instruction and live buffers; host FP state restored.
                unsafe { std::arch::asm!(
                    "mrs x8, fpcr", "mrs x9, fpsr", "msr fpcr, {control}", "msr fpsr, {initial}",
                    "ldr q29, [{left}]", "ldr q26, [{right}]", $instruction, "str q29, [{output}]",
                    "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                    control = in(reg) fpcr, initial = in(reg) 0x0800_0000u64,
                    left = in(reg) &left, right = in(reg) &right, output = in(reg) &mut native,
                    status = out(reg) status, out("x8") _, out("x9") _, out("v29") _, out("v26") _,
                    options(nostack, preserves_flags),
                ); }
                cpu.pc = 0; cpu.v[29] = left; cpu.v[26] = right; cpu.nzcv = 0xa;
                cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 0x0800_0000;
                cpu.step().unwrap();
                assert_eq!((cpu.v[29], cpu.sysregs.fpsr), (native, status),
                    "{} left={left:#x} right={right:#x} fpcr={fpcr:#x}", $instruction);
                assert_eq!(cpu.v[26], right);
                assert_eq!((cpu.pc, cpu.nzcv, cpu.sysregs.fpcr), (4, 0xa, fpcr));
            }}}
        }};
    }
    check!("fmul v29.2s, v29.2s, v26.s[0]", 0x0f9a93bd);
    check!("fmul v29.2s, v29.2s, v26.s[1]", 0x0fba93bd);
    check!("fmul v29.2s, v29.2s, v26.s[2]", 0x0f9a9bbd);
    check!("fmul v29.2s, v29.2s, v26.s[3]", 0x0fba9bbd);
    check!("fmul v29.4s, v29.4s, v26.s[0]", 0x4f9a93bd);
    check!("fmul v29.4s, v29.4s, v26.s[1]", 0x4fba93bd);
    check!("fmul v29.4s, v29.4s, v26.s[2]", 0x4f9a9bbd);
    check!("fmul v29.4s, v29.4s, v26.s[3]", 0x4fba9bbd);
    check!("fmul v29.2d, v29.2d, v26.d[0]", 0x4fda93bd);
    check!("fmul v29.2d, v29.2d, v26.d[1]", 0x4fda9bbd);
    check!("fmul s29, s29, v26.s[0]", 0x5f9a93bd);
    check!("fmul s29, s29, v26.s[1]", 0x5fba93bd);
    check!("fmul s29, s29, v26.s[2]", 0x5f9a9bbd);
    check!("fmul s29, s29, v26.s[3]", 0x5fba9bbd);
    check!("fmul d29, d29, v26.d[0]", 0x5fda93bd);
    check!("fmul d29, d29, v26.d[1]", 0x5fda9bbd);
}

#[test]
fn indexed_fmul_measured_alias_and_inactive_bits() {
    let mut cpu = cpu_for(0x4fda_93bd);
    cpu.v[29] = u128::from(2f64.to_bits()) | (u128::from(3f64.to_bits()) << 64);
    cpu.v[26] = u128::from(4f64.to_bits()) | (u128::from(9f64.to_bits()) << 64);
    cpu.step().unwrap();
    assert_eq!(
        cpu.v[29],
        u128::from(8f64.to_bits()) | (u128::from(12f64.to_bits()) << 64)
    );
    // Destination aliases the indexed source; capture its lane before any write.
    let mut cpu = cpu_for(0x4fda_93ba);
    cpu.v[29] = u128::from(2f64.to_bits()) | (u128::from(3f64.to_bits()) << 64);
    cpu.v[26] = u128::from(4f64.to_bits());
    cpu.step().unwrap();
    assert_eq!(
        cpu.v[26],
        u128::from(8f64.to_bits()) | (u128::from(12f64.to_bits()) << 64)
    );
    let mut cpu = cpu_for(0x0f9a_93bd);
    cpu.v[29] = u128::MAX << 64 | u128::from(2f32.to_bits()) | (u128::from(3f32.to_bits()) << 32);
    cpu.v[26] = u128::from(4f32.to_bits());
    cpu.step().unwrap();
    assert_eq!(
        cpu.v[29],
        u128::from(8f32.to_bits()) | (u128::from(12f32.to_bits()) << 32)
    );
}

#[test]
fn indexed_fmul_reserved_forms_reject_without_effects() {
    for word in [
        0x0fda_93bd,
        0x4ffa_93bd,
        0x1f9a_93bd,
        0x4f5a_93bd,
        0x6fda_93bd,
    ] {
        let mut cpu = cpu_for(word);
        cpu.v[29] = u128::MAX;
        cpu.sysregs.fpsr = 0x0800_0000;
        assert!(cpu.step().is_err(), "{word:#x}");
        assert_eq!(
            (cpu.pc, cpu.v[29], cpu.sysregs.fpsr),
            (0, u128::MAX, 0x0800_0000)
        );
    }
}

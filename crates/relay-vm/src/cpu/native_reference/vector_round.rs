use super::*;

#[test]
fn vector_round_measured_frinta_alias_and_inactive_lanes() {
    let mut cpu = cpu_for(0x2e21_8bff);
    cpu.v[31] = u128::from(0x3f00_0000u32)
        | (u128::from(0xbf00_0000u32) << 32)
        | (u128::from(0x7f80_0001u32) << 64);
    cpu.nzcv = 0xa;
    cpu.sysregs.fpcr = 3 << 22;
    cpu.sysregs.fpsr = 0x0800_0000;
    cpu.step().unwrap();
    assert_eq!(cpu.v[31], 0xbf80_0000_3f80_0000);
    assert_eq!(cpu.sysregs.fpsr, 0x0800_0000);
    assert_eq!((cpu.pc, cpu.nzcv, cpu.sysregs.fpcr), (4, 0xa, 3 << 22));
}

#[test]
fn vector_round_reserved_encodings_do_not_mutate_state() {
    for word in [0x0e61_8bff, 0x2e61_8bff, 0x2ea1_8bff] {
        let mut cpu = cpu_for(word);
        cpu.v[31] = u128::MAX;
        cpu.nzcv = 0xa;
        cpu.sysregs.fpsr = 0x0800_0000;
        assert!(cpu.step().is_err(), "reserved {word:#x}");
        assert_eq!(
            (cpu.v[31], cpu.pc, cpu.nzcv, cpu.sysregs.fpsr),
            (u128::MAX, 0, 0xa, 0x0800_0000)
        );
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_round_all_forms_match_native() {
    let mut inputs = Vec::new();
    for value in [
        0u32,
        1,
        0x007f_ffff,
        0x0080_0000,
        0x3eff_ffff,
        0x3f00_0000,
        0x3f00_0001,
        0x3fc0_0000,
        0x4020_0000,
        0x4aff_ffff,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7f80_1234,
        0x7fc0_5678,
    ] {
        for sign in [0, 1u32 << 31] {
            inputs.push(
                u128::from(value | sign)
                    | (u128::from(value ^ (1 << 31)) << 32)
                    | (u128::from(0x3f00_0000u32) << 64)
                    | (u128::from(0xbfc0_0000u32) << 96),
            );
        }
    }
    for value in [
        0u64,
        1,
        0x000f_ffff_ffff_ffff,
        0x0010_0000_0000_0000,
        0x3fdf_ffff_ffff_ffff,
        0x3fe0_0000_0000_0000,
        0x3fe0_0000_0000_0001,
        0x3ff8_0000_0000_0000,
        0x4004_0000_0000_0000,
        0x432f_ffff_ffff_ffff,
        0x7fef_ffff_ffff_ffff,
        0x7ff0_0000_0000_0000,
        0x7ff0_0000_0000_1234,
        0x7ff8_0000_0000_5678,
    ] {
        for sign in [0, 1u64 << 63] {
            inputs.push(u128::from(value | sign) | (u128::from(value ^ (1 << 63)) << 64));
        }
    }
    let mut random = 0x4798_bade_0147_ac13u64;
    for _ in 0..512 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let low = random;
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        inputs.push(u128::from(low) | (u128::from(random) << 64));
    }
    let mut comparisons = 0;
    macro_rules! check {
        ($instruction:literal, $word:expr) => {{
            let mut cpu = cpu_for($word);
            for &input in &inputs { for control in 0..16u64 {
                let fpcr = control << 22;
                let mut native = 0u128;
                let status: u64;
                // SAFETY: fixed instruction, live 16-byte buffers; restore host FP state.
                unsafe { std::arch::asm!(
                    "mrs x8, fpcr", "mrs x9, fpsr", "msr fpcr, {control}", "msr fpsr, {initial}",
                    "ldr q31, [{input}]", $instruction, "str q31, [{output}]",
                    "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                    control = in(reg) fpcr, initial = in(reg) 0x0800_0000u64,
                    input = in(reg) &input, output = in(reg) &mut native,
                    status = out(reg) status, out("x8") _, out("x9") _, out("v31") _,
                    options(nostack, preserves_flags),
                ); }
                cpu.pc = 0; cpu.v[31] = input; cpu.nzcv = 0xa;
                cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 0x0800_0000;
                cpu.step().unwrap();
                assert_eq!((cpu.v[31], cpu.sysregs.fpsr), (native, status),
                    "{} input={input:#x} fpcr={fpcr:#x}", $instruction);
                assert_eq!((cpu.pc, cpu.nzcv, cpu.sysregs.fpcr), (4, 0xa, fpcr));
                comparisons += 1;
            }}
        }};
    }
    check!("frintn v31.2s, v31.2s", 0x0e218bff);
    check!("frintn v31.4s, v31.4s", 0x4e218bff);
    check!("frintn v31.2d, v31.2d", 0x4e618bff);
    check!("frintp v31.2s, v31.2s", 0x0ea18bff);
    check!("frintp v31.4s, v31.4s", 0x4ea18bff);
    check!("frintp v31.2d, v31.2d", 0x4ee18bff);
    check!("frintm v31.2s, v31.2s", 0x0e219bff);
    check!("frintm v31.4s, v31.4s", 0x4e219bff);
    check!("frintm v31.2d, v31.2d", 0x4e619bff);
    check!("frintz v31.2s, v31.2s", 0x0ea19bff);
    check!("frintz v31.4s, v31.4s", 0x4ea19bff);
    check!("frintz v31.2d, v31.2d", 0x4ee19bff);
    check!("frinta v31.2s, v31.2s", 0x2e218bff);
    check!("frinta v31.4s, v31.4s", 0x6e218bff);
    check!("frinta v31.2d, v31.2d", 0x6e618bff);
    check!("frintx v31.2s, v31.2s", 0x2e219bff);
    check!("frintx v31.4s, v31.4s", 0x6e219bff);
    check!("frintx v31.2d, v31.2d", 0x6e619bff);
    check!("frinti v31.2s, v31.2s", 0x2ea19bff);
    check!("frinti v31.4s, v31.4s", 0x6ea19bff);
    check!("frinti v31.2d, v31.2d", 0x6ee19bff);
    assert_eq!(comparisons, 190_848);
}

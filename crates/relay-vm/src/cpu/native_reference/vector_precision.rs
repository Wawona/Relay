use super::*;

#[test]
fn vector_precision_measured_alias_and_lane_selection() {
    for (word, input, expected) in [
        (
            0x0e61_6bff,
            0xbff0_0000_0000_0000_3ff0_0000_0000_0000,
            0xbf80_0000_3f80_0000,
        ),
        (
            0x4e61_6bff,
            0xbff0_0000_0000_0000_3ff0_0000_0000_0000,
            0xbf80_0000_3f80_0000_3ff0_0000_0000_0000,
        ),
        (
            0x0e61_7bff,
            0x7f80_0001_7f80_0001_bf80_0000_3f80_0000,
            0xbff0_0000_0000_0000_3ff0_0000_0000_0000,
        ),
        (
            0x4e61_7bff,
            0xbf80_0000_3f80_0000_7f80_0001_7f80_0001,
            0xbff0_0000_0000_0000_3ff0_0000_0000_0000,
        ),
    ] {
        let mut cpu = cpu_for(word);
        cpu.v[31] = input;
        cpu.nzcv = 0xa;
        cpu.sysregs.fpcr = 3 << 22;
        cpu.sysregs.fpsr = 0x0800_0000;
        cpu.step().unwrap();
        assert_eq!(cpu.v[31], expected, "{word:#x}");
        assert_eq!(
            (cpu.pc, cpu.nzcv, cpu.sysregs.fpcr, cpu.sysregs.fpsr),
            (4, 0xa, 3 << 22, 0x0800_0000)
        );
    }
}

#[test]
fn vector_precision_unadvertised_half_does_not_mutate_state() {
    for word in [0x0e21_6bff, 0x4e21_6bff, 0x0e21_7bff, 0x4e21_7bff] {
        let mut cpu = cpu_for(word);
        cpu.v[31] = u128::MAX;
        cpu.nzcv = 0xa;
        cpu.sysregs.fpsr = 0x0800_0000;
        assert!(cpu.step().is_err(), "unadvertised {word:#x}");
        assert_eq!(
            (cpu.v[31], cpu.pc, cpu.nzcv, cpu.sysregs.fpsr),
            (u128::MAX, 0, 0xa, 0x0800_0000)
        );
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_precision_all_forms_match_native_controls_and_flags() {
    let mut inputs = Vec::new();
    for value in [
        0u64,
        1,
        0x000f_ffff_ffff_ffff,
        0x0010_0000_0000_0000,
        0x3690_0000_0000_0000,
        0x380f_ffff_ffff_ffff,
        0x3810_0000_0000_0000,
        0x3ff0_0000_0000_0000,
        0x3ff0_0000_1000_0000,
        0x47ef_ffff_f000_0000,
        0x7fef_ffff_ffff_ffff,
        0x7ff0_0000_0000_0000,
        0x7ff0_0000_0000_0001,
        0x7ff8_1234_5678_9abc,
    ] {
        for sign in [0, 1u64 << 63] {
            inputs.push(u128::from(value | sign) | (u128::from(value ^ (1 << 63)) << 64));
        }
    }
    for value in [
        0u32,
        1,
        0x007f_ffff,
        0x0080_0000,
        0x3f80_0000,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7f80_0001,
        0x7fc0_1234,
    ] {
        inputs.push(
            u128::from(value)
                | (u128::from(value ^ (1 << 31)) << 32)
                | (u128::from(value ^ 1) << 64)
                | (u128::from(value ^ 0x8000_0001) << 96),
        );
    }
    let mut random = 0x76bd_e841_23e5_1997u64;
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
        ($separate:literal, $aliased:literal, $word:expr) => {
            for source in &inputs {
                for mode in 0..4u64 {
                    for controls in [0, 1 << 24, 1 << 25, 3 << 24] {
                        let fpcr = (mode << 22) | controls;
                        for alias in [false, true] {
                            let old = if alias { *source } else { 0x1234_5678_9abc_def0u128 };
                            let mut native = 0u128;
                            let flags: u64;
                            let sticky = 0x0800_0000u64;
                            // SAFETY: fixed instructions, initialized vector storage,
                            // and host FPCR/FPSR restored within the same asm block.
                            unsafe {
                                if alias {
                                    std::arch::asm!(
                                        "ldr q0, [{source}]", "mrs x9, fpcr", "mrs x10, fpsr",
                                        "msr fpcr, {fpcr}", "msr fpsr, {sticky}", $aliased,
                                        "str q0, [{output}]", "mrs {flags}, fpsr",
                                        "msr fpcr, x9", "msr fpsr, x10",
                                        source = in(reg) source, output = in(reg) &mut native,
                                        fpcr = in(reg) fpcr, sticky = in(reg) sticky,
                                        flags = lateout(reg) flags,
                                        out("x9") _, out("x10") _, out("v0") _,
                                        options(nostack, preserves_flags));
                                } else {
                                    std::arch::asm!(
                                        "ldr q0, [{source}]", "ldr q1, [{old}]",
                                        "mrs x9, fpcr", "mrs x10, fpsr",
                                        "msr fpcr, {fpcr}", "msr fpsr, {sticky}", $separate,
                                        "str q1, [{output}]", "mrs {flags}, fpsr",
                                        "msr fpcr, x9", "msr fpsr, x10",
                                        source = in(reg) source, old = in(reg) &old,
                                        output = in(reg) &mut native, fpcr = in(reg) fpcr,
                                        sticky = in(reg) sticky, flags = lateout(reg) flags,
                                        out("x9") _, out("x10") _, out("v0") _, out("v1") _,
                                        options(nostack, preserves_flags));
                                }
                            }
                            let rd = if alias { 31 } else { 0 };
                            let mut cpu = cpu_for($word | (31 << 5) | rd);
                            cpu.v[31] = *source; cpu.v[rd as usize] = old;
                            cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = sticky; cpu.nzcv = 0xa;
                            cpu.step().unwrap();
                            assert_eq!((cpu.v[rd as usize], cpu.sysregs.fpsr), (native, flags),
                                       "{} alias={alias} source={source:#034x} fpcr={fpcr:#x}", $separate);
                            assert_eq!((cpu.pc, cpu.nzcv, cpu.sysregs.fpcr), (4, 0xa, fpcr));
                            if !alias { assert_eq!(cpu.v[31], *source); }
                            comparisons += 1;
                        }
                    }
                }
            }
        };
    }
    check!("fcvtn v1.2s, v0.2d", "fcvtn v0.2s, v0.2d", 0x0e61_6800);
    check!("fcvtn2 v1.4s, v0.2d", "fcvtn2 v0.4s, v0.2d", 0x4e61_6800);
    check!("fcvtl v1.2d, v0.2s", "fcvtl v0.2d, v0.2s", 0x0e61_7800);
    check!("fcvtl2 v1.2d, v0.4s", "fcvtl2 v0.2d, v0.4s", 0x4e61_7800);
    assert_eq!(comparisons, inputs.len() * 128);
    eprintln!("Vector precision: {comparisons} native comparisons");
}

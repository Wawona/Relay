//! Independently executed, statically assembled AArch64 reference cases.
//! No guest bytes are executed by the host and no executable memory is created.
use super::*;
use relay_core::{GuestPageSize, HostPageSize};

fn cpu_for(instruction: u32) -> StaticCpu {
    let mut memory =
        GuestMemory::allocate_on_host(GuestPageSize::FOUR_KIB, HostPageSize::SIXTEEN_KIB, 4096)
            .unwrap();
    memory.write(0, &instruction.to_le_bytes()).unwrap();
    StaticCpu::new(memory, 0).unwrap()
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn lane_moves_match_native_for_all_valid_widths_and_lanes() {
    let samples = [
        0u128,
        u128::MAX,
        0x8001_7ffe_fedc_ba98_7654_3210_89ab_cdef,
        0x1020_4080_0102_0408_1122_4488_55aa_55aa,
    ];
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for sample in samples {
                let native: u64;
                // SAFETY: source is an initialized 16-byte object; only the
                // statically named lane-move instruction runs on the host.
                unsafe {
                    std::arch::asm!(
                        "ldr q0, [{source}]", $instruction,
                        source = in(reg) &sample,
                        value = lateout(reg) native,
                        out("v0") _,
                        options(nostack, readonly, preserves_flags),
                    );
                }
                let mut cpu = cpu_for($word);
                cpu.v[30] = sample;
                cpu.set_x(0, u64::MAX);
                cpu.nzcv = 0xa;
                cpu.step().unwrap();
                assert_eq!(cpu.x(0), native, "{} source={sample:#034x}", $instruction);
                assert_eq!(cpu.v[30], sample);
                assert_eq!(cpu.nzcv, 0xa);
                assert_eq!(cpu.pc, 4);
            }
        };
    }
    check!("umov {value:w}, v0.b[0]", 0x0e013fc0);
    check!("umov {value:w}, v0.b[1]", 0x0e033fc0);
    check!("umov {value:w}, v0.b[2]", 0x0e053fc0);
    check!("umov {value:w}, v0.b[3]", 0x0e073fc0);
    check!("umov {value:w}, v0.b[4]", 0x0e093fc0);
    check!("umov {value:w}, v0.b[5]", 0x0e0b3fc0);
    check!("umov {value:w}, v0.b[6]", 0x0e0d3fc0);
    check!("umov {value:w}, v0.b[7]", 0x0e0f3fc0);
    check!("umov {value:w}, v0.b[8]", 0x0e113fc0);
    check!("umov {value:w}, v0.b[9]", 0x0e133fc0);
    check!("umov {value:w}, v0.b[10]", 0x0e153fc0);
    check!("umov {value:w}, v0.b[11]", 0x0e173fc0);
    check!("umov {value:w}, v0.b[12]", 0x0e193fc0);
    check!("umov {value:w}, v0.b[13]", 0x0e1b3fc0);
    check!("umov {value:w}, v0.b[14]", 0x0e1d3fc0);
    check!("umov {value:w}, v0.b[15]", 0x0e1f3fc0);
    check!("umov {value:w}, v0.h[0]", 0x0e023fc0);
    check!("umov {value:w}, v0.h[1]", 0x0e063fc0);
    check!("umov {value:w}, v0.h[2]", 0x0e0a3fc0);
    check!("umov {value:w}, v0.h[3]", 0x0e0e3fc0);
    check!("umov {value:w}, v0.h[4]", 0x0e123fc0);
    check!("umov {value:w}, v0.h[5]", 0x0e163fc0);
    check!("umov {value:w}, v0.h[6]", 0x0e1a3fc0);
    check!("umov {value:w}, v0.h[7]", 0x0e1e3fc0);
    check!("umov {value:w}, v0.s[0]", 0x0e043fc0);
    check!("umov {value:w}, v0.s[1]", 0x0e0c3fc0);
    check!("umov {value:w}, v0.s[2]", 0x0e143fc0);
    check!("umov {value:w}, v0.s[3]", 0x0e1c3fc0);
    check!("umov {value:x}, v0.d[0]", 0x4e083fc0);
    check!("umov {value:x}, v0.d[1]", 0x4e183fc0);
    check!("smov {value:w}, v0.b[0]", 0x0e012fc0);
    check!("smov {value:w}, v0.b[1]", 0x0e032fc0);
    check!("smov {value:w}, v0.b[2]", 0x0e052fc0);
    check!("smov {value:w}, v0.b[3]", 0x0e072fc0);
    check!("smov {value:w}, v0.b[4]", 0x0e092fc0);
    check!("smov {value:w}, v0.b[5]", 0x0e0b2fc0);
    check!("smov {value:w}, v0.b[6]", 0x0e0d2fc0);
    check!("smov {value:w}, v0.b[7]", 0x0e0f2fc0);
    check!("smov {value:w}, v0.b[8]", 0x0e112fc0);
    check!("smov {value:w}, v0.b[9]", 0x0e132fc0);
    check!("smov {value:w}, v0.b[10]", 0x0e152fc0);
    check!("smov {value:w}, v0.b[11]", 0x0e172fc0);
    check!("smov {value:w}, v0.b[12]", 0x0e192fc0);
    check!("smov {value:w}, v0.b[13]", 0x0e1b2fc0);
    check!("smov {value:w}, v0.b[14]", 0x0e1d2fc0);
    check!("smov {value:w}, v0.b[15]", 0x0e1f2fc0);
    check!("smov {value:w}, v0.h[0]", 0x0e022fc0);
    check!("smov {value:w}, v0.h[1]", 0x0e062fc0);
    check!("smov {value:w}, v0.h[2]", 0x0e0a2fc0);
    check!("smov {value:w}, v0.h[3]", 0x0e0e2fc0);
    check!("smov {value:w}, v0.h[4]", 0x0e122fc0);
    check!("smov {value:w}, v0.h[5]", 0x0e162fc0);
    check!("smov {value:w}, v0.h[6]", 0x0e1a2fc0);
    check!("smov {value:w}, v0.h[7]", 0x0e1e2fc0);
    check!("smov {value:x}, v0.b[0]", 0x4e012fc0);
    check!("smov {value:x}, v0.b[1]", 0x4e032fc0);
    check!("smov {value:x}, v0.b[2]", 0x4e052fc0);
    check!("smov {value:x}, v0.b[3]", 0x4e072fc0);
    check!("smov {value:x}, v0.b[4]", 0x4e092fc0);
    check!("smov {value:x}, v0.b[5]", 0x4e0b2fc0);
    check!("smov {value:x}, v0.b[6]", 0x4e0d2fc0);
    check!("smov {value:x}, v0.b[7]", 0x4e0f2fc0);
    check!("smov {value:x}, v0.b[8]", 0x4e112fc0);
    check!("smov {value:x}, v0.b[9]", 0x4e132fc0);
    check!("smov {value:x}, v0.b[10]", 0x4e152fc0);
    check!("smov {value:x}, v0.b[11]", 0x4e172fc0);
    check!("smov {value:x}, v0.b[12]", 0x4e192fc0);
    check!("smov {value:x}, v0.b[13]", 0x4e1b2fc0);
    check!("smov {value:x}, v0.b[14]", 0x4e1d2fc0);
    check!("smov {value:x}, v0.b[15]", 0x4e1f2fc0);
    check!("smov {value:x}, v0.h[0]", 0x4e022fc0);
    check!("smov {value:x}, v0.h[1]", 0x4e062fc0);
    check!("smov {value:x}, v0.h[2]", 0x4e0a2fc0);
    check!("smov {value:x}, v0.h[3]", 0x4e0e2fc0);
    check!("smov {value:x}, v0.h[4]", 0x4e122fc0);
    check!("smov {value:x}, v0.h[5]", 0x4e162fc0);
    check!("smov {value:x}, v0.h[6]", 0x4e1a2fc0);
    check!("smov {value:x}, v0.h[7]", 0x4e1e2fc0);
    check!("smov {value:x}, v0.s[0]", 0x4e042fc0);
    check!("smov {value:x}, v0.s[1]", 0x4e0c2fc0);
    check!("smov {value:x}, v0.s[2]", 0x4e142fc0);
    check!("smov {value:x}, v0.s[3]", 0x4e1c2fc0);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn precision_conversion_matches_native_controls_results_and_flags() {
    let mut samples = vec![
        0u64,
        1,
        u64::MAX,
        0x8000_0000_0000_0000,
        0x3ff0_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff0_0000_0000_0001,
        0x7ff8_1234_5678_9abc,
        (f32::MAX as f64).to_bits(),
        (f32::MIN_POSITIVE as f64).to_bits(),
    ];
    // Test both sides of exponent/rounding boundaries and exact ties.
    for exponent in [
        0u64, 1, 872, 873, 874, 896, 897, 898, 1023, 1150, 1151, 2046,
    ] {
        for fraction in [
            0u64,
            1,
            (1 << 28) - 1,
            1 << 28,
            (1 << 28) + 1,
            (1 << 29) - 1,
            1 << 29,
            (1 << 52) - 1,
        ] {
            for sign in [0, 1u64 << 63] {
                samples.push(sign | (exponent << 52) | fraction);
            }
        }
    }
    let mut random = 0xdead_beef_1234_5678u64;
    for _ in 0..256 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        samples.push(random);
    }
    let mut cpu = cpu_for(0x1e62_43ff); // FCVT S31,D31, actual Stage 2 failure.
    for control in 0..32u64 {
        let fpcr = control << 22; // RMode, FZ, DN, AHP (irrelevant to S/D).
        for &input in &samples {
            let native: u64;
            let status: u64;
            // SAFETY: fixed instructions, no memory access, host FP control
            // and status are saved/restored within one assembly block.
            unsafe {
                std::arch::asm!(
                    "mrs x8, fpcr", "mrs x9, fpsr",
                    "msr fpcr, {control}", "msr fpsr, {initial}",
                    "fmov d0, {input}", "fcvt s1, d0", "fmov w10, s1",
                    "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                    "mov {result}, x10",
                    control = in(reg) fpcr, initial = in(reg) 2u64,
                    input = in(reg) input, status = out(reg) status,
                    result = out(reg) native,
                    out("x8") _, out("x9") _, out("x10") _,
                    out("v0") _, out("v1") _,
                    options(nostack, nomem, preserves_flags),
                );
            }
            cpu.pc = 0;
            cpu.v[31] = u128::from(input) | (u128::from(u64::MAX) << 64);
            cpu.sysregs.fpcr = fpcr;
            cpu.sysregs.fpsr = 2;
            cpu.step().unwrap();
            assert_eq!(
                (cpu.v[31], cpu.sysregs.fpsr),
                (u128::from(native), status),
                "input={input:#018x} fpcr={fpcr:#010x}"
            );
            assert_eq!(cpu.sysregs.fpcr, fpcr);
        }
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn widening_conversion_matches_native_controls_results_and_flags() {
    let mut samples = vec![
        0u32,
        1,
        u32::MAX,
        0x8000_0000,
        0x3f80_0000,
        0x7f80_0000,
        0xff80_0000,
        0x7f80_0001,
        0x7fc1_2345,
    ];
    for exponent in 0..256u32 {
        for fraction in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_ffff] {
            for sign in [0, 1 << 31] {
                samples.push(sign | (exponent << 23) | fraction);
            }
        }
    }
    let mut cpu = cpu_for(0x1e22_c3ff); // FCVT D31,S31.
    for control in 0..32u64 {
        let fpcr = control << 22;
        for &input in &samples {
            let native: u64;
            let status: u64;
            // SAFETY: fixed instructions, no memory access, complete host
            // floating-point environment restored before leaving this block.
            unsafe {
                std::arch::asm!(
                    "mrs x8, fpcr", "mrs x9, fpsr",
                    "msr fpcr, {control}", "msr fpsr, {initial}",
                    "fmov s0, {input:w}", "fcvt d1, s0", "fmov x10, d1",
                    "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                    "mov {result}, x10",
                    control = in(reg) fpcr, initial = in(reg) 2u64,
                    input = in(reg) input, status = out(reg) status,
                    result = out(reg) native,
                    out("x8") _, out("x9") _, out("x10") _,
                    out("v0") _, out("v1") _,
                    options(nostack, nomem, preserves_flags),
                );
            }
            cpu.pc = 0;
            cpu.v[31] = u128::from(input) | (!0u128 << 32);
            cpu.sysregs.fpcr = fpcr;
            cpu.sysregs.fpsr = 2;
            cpu.step().unwrap();
            assert_eq!(
                (cpu.v[31], cpu.sysregs.fpsr),
                (u128::from(native), status),
                "input={input:#010x} fpcr={fpcr:#010x}"
            );
        }
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn doubleword_modified_immediate_matches_native_both_vector_widths() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            let mut native = [0u8; 16];
            // SAFETY: static MOVI instruction and a 16-byte output buffer.
            unsafe { std::arch::asm!(
                $instruction, "str q0, [{output}]",
                output = in(reg) native.as_mut_ptr(), out("v0") _,
                options(nostack, preserves_flags),
            ); }
            let mut cpu = cpu_for($word);
            cpu.v[25] = u128::MAX;
            cpu.step().unwrap();
            assert_eq!(cpu.v[25].to_le_bytes(), native, "{}", $instruction);
        };
    }
    check!("movi d0, #0x0000000000000000", 0x2f00e419);
    check!("movi v0.2d, #0x0000000000000000", 0x6f00e419);
    check!("movi d0, #0xffffffffffffffff", 0x2f07e7f9);
    check!("movi v0.2d, #0xffffffffffffffff", 0x6f07e7f9);
    check!("movi d0, #0x00ff00ff00ff00ff", 0x2f02e6b9);
    check!("movi v0.2d, #0x00ff00ff00ff00ff", 0x6f02e6b9);
    check!("movi d0, #0xff00ff00ff00ff00", 0x2f05e559);
    check!("movi v0.2d, #0xff00ff00ff00ff00", 0x6f05e559);
    check!("movi d0, #0x00000000000000ff", 0x2f00e439);
    check!("movi v0.2d, #0x00000000000000ff", 0x6f00e439);
    check!("movi d0, #0x000000000000ff00", 0x2f00e459);
    check!("movi v0.2d, #0x000000000000ff00", 0x6f00e459);
    check!("movi d0, #0x0000000000ff0000", 0x2f00e499);
    check!("movi v0.2d, #0x0000000000ff0000", 0x6f00e499);
    check!("movi d0, #0x00000000ff000000", 0x2f00e519);
    check!("movi v0.2d, #0x00000000ff000000", 0x6f00e519);
    check!("movi d0, #0x000000ff00000000", 0x2f00e619);
    check!("movi v0.2d, #0x000000ff00000000", 0x6f00e619);
    check!("movi d0, #0x0000ff0000000000", 0x2f01e419);
    check!("movi v0.2d, #0x0000ff0000000000", 0x6f01e419);
    check!("movi d0, #0x00ff000000000000", 0x2f02e419);
    check!("movi v0.2d, #0x00ff000000000000", 0x6f02e419);
    check!("movi d0, #0xff00000000000000", 0x2f04e419);
    check!("movi v0.2d, #0xff00000000000000", 0x6f04e419);
}

#[test]
fn reserved_lane_moves_fail_without_changing_architectural_state() {
    for word in [
        0x0e00_3fc0,
        0x4e04_3fc0,
        0x0e08_3fc0,
        0x0e04_2fc0,
        0x4e08_2fc0,
    ] {
        let mut cpu = cpu_for(word);
        cpu.x[0] = 42;
        cpu.v[30] = u128::MAX;
        cpu.nzcv = 9;
        assert!(cpu.step().is_err(), "reserved encoding {word:#010x}");
        assert_eq!(
            (cpu.x[0], cpu.v[30], cpu.nzcv, cpu.pc),
            (42, u128::MAX, 9, 0)
        );
    }
}

#[test]
fn reserved_rounding_encodings_reject_without_mutation() {
    for word in [0x1e26_c001, 0x1e66_c001, 0x1ea4_4001, 0x1ee4_4001] {
        let mut cpu = cpu_for(word);
        cpu.v = [u128::MAX; 32];
        cpu.sysregs.fpcr = 0x0300_0000;
        cpu.sysregs.fpsr = 0x9f;
        cpu.nzcv = 0xa;
        assert!(cpu.step().is_err(), "word={word:#x}");
        assert_eq!(cpu.v, [u128::MAX; 32]);
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.sysregs.fpcr, 0x0300_0000);
        assert_eq!(cpu.sysregs.fpsr, 0x9f);
        assert_eq!(cpu.nzcv, 0xa);
    }
}

#[test]
fn scalar_rounding_in_place_clears_inactive_bits_and_keeps_prior_status() {
    let mut cpu = cpu_for(0x1e65_4000); // FRINTM D0,D0, the measured boot miss.
    cpu.v[0] = (u128::from(u64::MAX) << 64) | 0xbfe0_0000_0000_0000;
    cpu.sysregs.fpsr = 0x0800_0010;
    cpu.step().unwrap();
    assert_eq!(cpu.v[0], 0xbff0_0000_0000_0000);
    assert_eq!(cpu.sysregs.fpsr, 0x0800_0010);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn widening_add_reads_aliased_sources_before_writing() {
    let mut cpu = cpu_for(0x0ebe_13ff); // SADDW V31.2D,V31.2D,V30.2S.
    cpu.v[31] = (u128::from(u64::MAX) << 64) | 1;
    cpu.v[30] = 0x0000_0002_ffff_ffff;
    cpu.step().unwrap();
    assert_eq!(cpu.v[31], 1u128 << 64);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn immediate_shift_boundaries_preserve_insert_bits_and_round_without_overflow() {
    let cases: [(u32, u64, u64, u64); 4] = [
        (0x7f40_2402, u64::MAX, 0, 1),           // URSHR D2,D0,#64.
        (0x5f40_2402, u64::MAX, 0, 0),           // SRSHR D2,D0,#64.
        (0x7f40_4402, u64::MAX, 0x1234, 0x1234), // SRI #64 preserves D2.
        (0x7f60_5402, 1, 0x1234, 0x1_0000_1234), // SLI preserves low bits.
    ];
    for (word, source, prior, expected) in cases {
        let mut cpu = cpu_for(word);
        cpu.v[0] = u128::from(source);
        cpu.v[2] = u128::from(prior) | (u128::from(u64::MAX) << 64);
        cpu.step().unwrap();
        assert_eq!(cpu.v[2], u128::from(expected));
    }
    let mut cpu = cpu_for(0x7f60_5400); // SLI D0,D0,#32 reads the old D0.
    cpu.v[0] = 0x1234_5678_9abc_def0;
    cpu.step().unwrap();
    assert_eq!(cpu.v[0], 0x9abc_def0_9abc_def0);
}

#[test]
fn structure_lane_load_preserves_other_bits_and_replication_clears_upper_half() {
    let mut cpu = cpu_for(0x0d40_041f); // LD1 {V31.B}[1],[X0], measured boot miss.
    cpu.set_x(0, 0x100);
    cpu.memory.write(0x100, &[0x42]).unwrap();
    cpu.v[31] = u128::MAX;
    cpu.step().unwrap();
    assert_eq!(cpu.v[31], (u128::MAX & !0xff00) | 0x4200);
    let mut cpu = cpu_for(0x0d40_cc20); // LD1R {V0.1D},[X1].
    cpu.set_x(1, 0x100);
    cpu.memory
        .write(0x100, &0x0123_4567_89ab_cdefu64.to_le_bytes())
        .unwrap();
    cpu.v[0] = u128::MAX;
    cpu.step().unwrap();
    assert_eq!(cpu.v[0], 0x0123_4567_89ab_cdef);
}

#[test]
fn scalar_dup_in_place_reads_upper_lane_before_clearing_it() {
    let mut cpu = cpu_for(0x5e18_079c); // DUP D28,V28.D[1], measured systemd miss.
    cpu.v[28] = (0x1234_5678_9abc_def0u128 << 64) | u128::from(u64::MAX);
    cpu.step().unwrap();
    assert_eq!(cpu.v[28], 0x1234_5678_9abc_def0);
}

#[test]
fn lane_move_to_zero_register_does_not_change_stack_pointer() {
    let mut cpu = cpu_for(0x4e08_3fdf); // UMOV XZR,V30.D[0].
    cpu.v[30] = u128::MAX;
    cpu.sp = 0x1234;
    cpu.step().unwrap();
    assert_eq!(cpu.sp, 0x1234);
    assert_eq!(cpu.x(31), 0);
    assert_eq!(cpu.pc, 4);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn integer_to_float_matches_native_width_sign_rounding_and_status() {
    let mut samples = vec![
        0u64,
        1,
        u64::MAX,
        i64::MIN as u64,
        i64::MAX as u64,
        u64::from(u32::MAX),
        u64::from(i32::MAX as u32),
        1 << 31,
    ];
    for power in 1..64 {
        for delta in [0, 1, 3] {
            let value = (1u64 << power).wrapping_add(delta);
            samples.extend([value, value.wrapping_neg()]);
        }
    }
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            let mut cpu = cpu_for($word);
            for mode in 0..4u64 {
                let fpcr = mode << 22;
                for &input in &samples {
                    let native: u64;
                    let status: u64;
                    // SAFETY: fixed conversion and complete host FP-state
                    // save/restore in one block, without memory access.
                    unsafe { std::arch::asm!(
                        "mrs x8, fpcr", "mrs x9, fpsr", "msr fpcr, {control}",
                        "msr fpsr, {initial}", $instruction,
                        "fmov x10, d1", "mrs {status}, fpsr",
                        "msr fpcr, x8", "msr fpsr, x9", "mov {result}, x10",
                        control = in(reg) fpcr, initial = in(reg) 2u64,
                        input = in(reg) input, status = out(reg) status,
                        result = out(reg) native,
                        out("x8") _, out("x9") _, out("x10") _, out("v1") _,
                        options(nostack, nomem, preserves_flags),
                    ); }
                    cpu.pc = 0; cpu.set_x(21, input); cpu.v[31] = u128::MAX;
                    cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 2;
                    cpu.step().unwrap();
                    assert_eq!((cpu.v[31], cpu.sysregs.fpsr), (u128::from(native), status),
                        "{} input={input:#018x} fpcr={fpcr:#x}", $instruction);
                }
            }
        };
    }
    check!("scvtf s1, {input:w}", 0x1e22_02bf);
    check!("ucvtf s1, {input:w}", 0x1e23_02bf);
    check!("scvtf d1, {input:w}", 0x1e62_02bf);
    check!("ucvtf d1, {input:w}", 0x1e63_02bf);
    check!("scvtf s1, {input:x}", 0x9e22_02bf);
    check!("ucvtf s1, {input:x}", 0x9e23_02bf);
    check!("scvtf d1, {input:x}", 0x9e62_02bf);
    check!("ucvtf d1, {input:x}", 0x9e63_02bf);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_duplicate_matches_native_every_element_and_lane() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for source in [0u128, u128::MAX, 0x8001_7ffe_fedc_ba98_7654_3210_89ab_cdef] {
                let mut native = [0u8; 16];
                // SAFETY: fixed instruction with 16-byte source/output arrays.
                unsafe { std::arch::asm!(
                    "ldr q1, [{source}]", $instruction, "str q0, [{output}]",
                    source = in(reg) &source, output = in(reg) native.as_mut_ptr(),
                    out("v0") _, out("v1") _, options(nostack, preserves_flags),
                ); }
                let mut cpu = cpu_for($word);
                cpu.v[25] = source; cpu.v[29] = u128::MAX;
                cpu.step().unwrap();
                assert_eq!(cpu.v[29].to_le_bytes(), native, "{}", $instruction);
                assert_eq!(cpu.v[25], source);
            }
        };
    }
    check!("dup v0.8b, v1.b[0]", 0x0e01073d);
    check!("dup v0.8b, v1.b[1]", 0x0e03073d);
    check!("dup v0.8b, v1.b[2]", 0x0e05073d);
    check!("dup v0.8b, v1.b[3]", 0x0e07073d);
    check!("dup v0.8b, v1.b[4]", 0x0e09073d);
    check!("dup v0.8b, v1.b[5]", 0x0e0b073d);
    check!("dup v0.8b, v1.b[6]", 0x0e0d073d);
    check!("dup v0.8b, v1.b[7]", 0x0e0f073d);
    check!("dup v0.8b, v1.b[8]", 0x0e11073d);
    check!("dup v0.8b, v1.b[9]", 0x0e13073d);
    check!("dup v0.8b, v1.b[10]", 0x0e15073d);
    check!("dup v0.8b, v1.b[11]", 0x0e17073d);
    check!("dup v0.8b, v1.b[12]", 0x0e19073d);
    check!("dup v0.8b, v1.b[13]", 0x0e1b073d);
    check!("dup v0.8b, v1.b[14]", 0x0e1d073d);
    check!("dup v0.8b, v1.b[15]", 0x0e1f073d);
    check!("dup v0.4h, v1.h[0]", 0x0e02073d);
    check!("dup v0.4h, v1.h[1]", 0x0e06073d);
    check!("dup v0.4h, v1.h[2]", 0x0e0a073d);
    check!("dup v0.4h, v1.h[3]", 0x0e0e073d);
    check!("dup v0.4h, v1.h[4]", 0x0e12073d);
    check!("dup v0.4h, v1.h[5]", 0x0e16073d);
    check!("dup v0.4h, v1.h[6]", 0x0e1a073d);
    check!("dup v0.4h, v1.h[7]", 0x0e1e073d);
    check!("dup v0.2s, v1.s[0]", 0x0e04073d);
    check!("dup v0.2s, v1.s[1]", 0x0e0c073d);
    check!("dup v0.2s, v1.s[2]", 0x0e14073d);
    check!("dup v0.2s, v1.s[3]", 0x0e1c073d);
    check!("dup v0.16b, v1.b[0]", 0x4e01073d);
    check!("dup v0.16b, v1.b[1]", 0x4e03073d);
    check!("dup v0.16b, v1.b[2]", 0x4e05073d);
    check!("dup v0.16b, v1.b[3]", 0x4e07073d);
    check!("dup v0.16b, v1.b[4]", 0x4e09073d);
    check!("dup v0.16b, v1.b[5]", 0x4e0b073d);
    check!("dup v0.16b, v1.b[6]", 0x4e0d073d);
    check!("dup v0.16b, v1.b[7]", 0x4e0f073d);
    check!("dup v0.16b, v1.b[8]", 0x4e11073d);
    check!("dup v0.16b, v1.b[9]", 0x4e13073d);
    check!("dup v0.16b, v1.b[10]", 0x4e15073d);
    check!("dup v0.16b, v1.b[11]", 0x4e17073d);
    check!("dup v0.16b, v1.b[12]", 0x4e19073d);
    check!("dup v0.16b, v1.b[13]", 0x4e1b073d);
    check!("dup v0.16b, v1.b[14]", 0x4e1d073d);
    check!("dup v0.16b, v1.b[15]", 0x4e1f073d);
    check!("dup v0.8h, v1.h[0]", 0x4e02073d);
    check!("dup v0.8h, v1.h[1]", 0x4e06073d);
    check!("dup v0.8h, v1.h[2]", 0x4e0a073d);
    check!("dup v0.8h, v1.h[3]", 0x4e0e073d);
    check!("dup v0.8h, v1.h[4]", 0x4e12073d);
    check!("dup v0.8h, v1.h[5]", 0x4e16073d);
    check!("dup v0.8h, v1.h[6]", 0x4e1a073d);
    check!("dup v0.8h, v1.h[7]", 0x4e1e073d);
    check!("dup v0.4s, v1.s[0]", 0x4e04073d);
    check!("dup v0.4s, v1.s[1]", 0x4e0c073d);
    check!("dup v0.4s, v1.s[2]", 0x4e14073d);
    check!("dup v0.4s, v1.s[3]", 0x4e1c073d);
    check!("dup v0.2d, v1.d[0]", 0x4e08073d);
    check!("dup v0.2d, v1.d[1]", 0x4e18073d);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn scalar_arithmetic_matches_native_results_and_flags() {
    macro_rules! check {
        ($op:literal, $load0:literal, $load1:literal, $store:literal, $word:expr, $double:expr) => {{
            let mut cpu = cpu_for($word);
            let special: &[u64] = if $double {
                &[0, 1, 0x000f_ffff_ffff_ffff, 0x0010_0000_0000_0000,
                  0x0010_0000_0000_0001, 0x3fe0_0000_0000_0000, 0x3fef_ffff_ffff_ffff, 0x3ff0_0000_0000_0000,
                  0x3ff0_0000_0000_0001, 0x4008_0000_0000_0000, 0x7fef_ffff_ffff_ffff,
                  0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_1234, 0x7ff8_0000_0000_5678]
            } else {
                &[0, 1, 0x007f_ffff, 0x0080_0000, 0x0080_0001, 0x3f00_0000, 0x3f7f_ffff,
                  0x3f80_0000, 0x3f80_0001, 0x4040_0000, 0x7f7f_ffff,
                  0x7f80_0000, 0x7f80_1234, 0x7fc0_5678]
            };
            let sign = if $double { 1u64 << 63 } else { 1u64 << 31 };
            let mut pairs = Vec::new();
            for &left in special { for &right in special {
                for signs in 0..4 {
                    pairs.push((left | if signs & 1 != 0 { sign } else { 0 },
                                right | if signs & 2 != 0 { sign } else { 0 }));
                }
            }}
            let mut random = 0x83ca_1307_450b_deadu64;
            for _ in 0..4096 {
                let left = random;
                random ^= random << 13; random ^= random >> 7; random ^= random << 17;
                pairs.push((left, random));
            }
            for control in 0..16u64 {
                let fpcr = control << 22;
                for &(left, right) in &pairs {
                    let native: u64;
                    let status: u64;
                    // SAFETY: fixed instructions, with host FP state restored
                    // before leaving the assembly block. No guest bytes run.
                    unsafe { std::arch::asm!(
                        "mrs x8, fpcr", "mrs x9, fpsr",
                        "msr fpcr, {control}", "msr fpsr, {initial}",
                        $load0, $load1, $op, $store,
                        "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                        "mov {result}, x10",
                        control = in(reg) fpcr, initial = in(reg) 0x0800_0000u64,
                        left = in(reg) left, right = in(reg) right,
                        status = out(reg) status, result = out(reg) native,
                        out("x8") _, out("x9") _, out("x10") _,
                        out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, nomem, preserves_flags),
                    ); }
                    cpu.pc = 0;
                    cpu.v[0] = u128::from(left);
                    cpu.v[1] = u128::from(right);
                    cpu.v[2] = u128::MAX;
                    cpu.sysregs.fpcr = fpcr;
                    cpu.sysregs.fpsr = 0x0800_0000;
                    cpu.step().unwrap();
                    assert_eq!((cpu.v[2], cpu.sysregs.fpsr), (u128::from(native), status),
                        "{} left={left:#018x} right={right:#018x} fpcr={fpcr:#x}", $op);
                }
            }
        }};
    }
    check!(
        "fdiv s2, s0, s1",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        "fmov w10, s2",
        0x1e21_1802,
        false
    );
    check!(
        "fadd s2, s0, s1",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        "fmov w10, s2",
        0x1e21_2802,
        false
    );
    check!(
        "fsub s2, s0, s1",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        "fmov w10, s2",
        0x1e21_3802,
        false
    );
    check!(
        "fmul s2, s0, s1",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        "fmov w10, s2",
        0x1e21_0802,
        false
    );
    check!(
        "fdiv d2, d0, d1",
        "fmov d0, {left}",
        "fmov d1, {right}",
        "fmov x10, d2",
        0x1e61_1802,
        true
    );
    check!(
        "fadd d2, d0, d1",
        "fmov d0, {left}",
        "fmov d1, {right}",
        "fmov x10, d2",
        0x1e61_2802,
        true
    );
    check!(
        "fsub d2, d0, d1",
        "fmov d0, {left}",
        "fmov d1, {right}",
        "fmov x10, d2",
        0x1e61_3802,
        true
    );
    check!(
        "fmul d2, d0, d1",
        "fmov d0, {left}",
        "fmov d1, {right}",
        "fmov x10, d2",
        0x1e61_0802,
        true
    );
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn general_register_broadcast_matches_native_all_widths() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for input in [0u64, u64::MAX, 0x8040_2010_0804_0201, 0x0123_4567_89ab_cdef] {
                let mut native = 0u128;
                // SAFETY: output is a valid 16-byte object. Only fixed DUP
                // instructions execute, with no guest instruction execution.
                unsafe { std::arch::asm!(
                    $instruction, "str q0, [{output}]",
                    input = in(reg) input, output = in(reg) &mut native,
                    out("v0") _, options(nostack, preserves_flags),
                ); }
                let mut cpu = cpu_for($word);
                cpu.set_x(5, input);
                cpu.v[30] = u128::MAX;
                cpu.step().unwrap();
                assert_eq!(cpu.v[30], native, "{} input={input:#x}", $instruction);
            }
        };
    }
    check!("dup v0.2d, {input:x}", 0x4e08_0cbe);
    check!("dup v0.8b, {input:w}", 0x0e01_0cbe);
    check!("dup v0.16b, {input:w}", 0x4e01_0cbe);
    check!("dup v0.4h, {input:w}", 0x0e02_0cbe);
    check!("dup v0.8h, {input:w}", 0x4e02_0cbe);
    check!("dup v0.2s, {input:w}", 0x0e04_0cbe);
    check!("dup v0.4s, {input:w}", 0x4e04_0cbe);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn floating_comparisons_match_native_flags_and_exceptions() {
    macro_rules! check {
        ($op:literal, $load0:literal, $load1:literal, $word:expr, $double:expr) => {{
            let mut cpu = cpu_for($word);
            let special: &[u64] = if $double {
                &[0, 1, 0x000f_ffff_ffff_ffff, 0x0010_0000_0000_0000,
                  0x3ff0_0000_0000_0000, 0x7ff0_0000_0000_0000,
                  0x7ff0_0000_0000_1234, 0x7ff8_0000_0000_5678]
            } else { &[0, 1, 0x007f_ffff, 0x0080_0000, 0x3f80_0000, 0x7f80_0000, 0x7f80_1234, 0x7fc0_5678] };
            let sign = if $double { 1u64 << 63 } else { 1u64 << 31 };
            for control in 0..16u64 { for &a in special { for &b in special { for signs in 0..4 {
                let left = a | if signs & 1 != 0 { sign } else { 0 };
                let right = b | if signs & 2 != 0 { sign } else { 0 };
                let fpcr = control << 22;
                let flags: u64;
                let status: u64;
                // SAFETY: fixed instructions and all host control/status restored.
                unsafe { std::arch::asm!(
                    "mrs x8, fpcr", "mrs x9, fpsr", "mrs x11, nzcv",
                    "msr fpcr, {control}", "msr fpsr, {initial}",
                    $load0, $load1, $op, "mrs {flags}, nzcv", "mrs {status}, fpsr",
                    "msr fpcr, x8", "msr fpsr, x9", "msr nzcv, x11",
                    control = in(reg) fpcr, initial = in(reg) 2u64,
                    left = in(reg) left, right = in(reg) right,
                    status = out(reg) status, flags = out(reg) flags,
                    out("x8") _, out("x9") _, out("x11") _, out("v0") _, out("v1") _,
                    options(nostack, nomem, preserves_flags),
                ); }
                cpu.pc = 0; cpu.v[0] = u128::from(left); cpu.v[1] = u128::from(right);
                cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 2;
                cpu.step().unwrap();
                assert_eq!((cpu.nzcv, cpu.sysregs.fpsr), ((flags >> 28) as u8, status),
                    "{} left={left:#x} right={right:#x} fpcr={fpcr:#x}", $op);
            }}}}
        }};
    }
    check!(
        "fcmp d0, d1",
        "fmov d0, {left}",
        "fmov d1, {right}",
        0x1e61_2000,
        true
    );
    check!(
        "fcmpe d0, d1",
        "fmov d0, {left}",
        "fmov d1, {right}",
        0x1e61_2010,
        true
    );
    check!(
        "fcmp d0, #0.0",
        "fmov d0, {left}",
        "fmov d1, {right}",
        0x1e60_2008,
        true
    );
    check!(
        "fcmpe d0, #0.0",
        "fmov d0, {left}",
        "fmov d1, {right}",
        0x1e60_2018,
        true
    );
    check!(
        "fcmp s0, s1",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        0x1e21_2000,
        false
    );
    check!(
        "fcmpe s0, s1",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        0x1e21_2010,
        false
    );
    check!(
        "fcmp s0, #0.0",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        0x1e20_2008,
        false
    );
    check!(
        "fcmpe s0, #0.0",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        0x1e20_2018,
        false
    );
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_add_subtract_match_native_lane_wrapping() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01] {
                for right in [1u128, u128::MAX, 0x0101_0101_0101_0101_0101_0101_0101_0101] {
                    let mut native = 0u128;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("add d2, d0, d1", 0x5ee1_8402);
    check!("sub d2, d0, d1", 0x7ee1_8402);
    check!("add v2.2d, v0.2d, v1.2d", 0x4ee1_8402);
    check!("add v2.8b, v0.8b, v1.8b", 0x0e21_8402);
    check!("add v2.16b, v0.16b, v1.16b", 0x4e21_8402);
    check!("add v2.4h, v0.4h, v1.4h", 0x0e61_8402);
    check!("add v2.8h, v0.8h, v1.8h", 0x4e61_8402);
    check!("add v2.2s, v0.2s, v1.2s", 0x0ea1_8402);
    check!("add v2.4s, v0.4s, v1.4s", 0x4ea1_8402);
    check!("sub v2.2d, v0.2d, v1.2d", 0x6ee1_8402);
    check!("sub v2.8b, v0.8b, v1.8b", 0x2e21_8402);
    check!("sub v2.16b, v0.16b, v1.16b", 0x6e21_8402);
    check!("sub v2.4h, v0.4h, v1.4h", 0x2e61_8402);
    check!("sub v2.8h, v0.8h, v1.8h", 0x6e61_8402);
    check!("sub v2.2s, v0.2s, v1.2s", 0x2ea1_8402);
    check!("sub v2.4s, v0.4s, v1.4s", 0x6ea1_8402);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn float_to_integer_matches_native_rounding_and_saturation() {
    macro_rules! check {
        ($op:literal, $load:literal, $word:expr, $double:expr) => {{
            let mut cpu = cpu_for($word);
            let mut samples = Vec::new();
            let (fraction_bits, bias, sign, max_exp) = if $double { (52, 1023, 1u64 << 63, 2047) }
                else { (23, 127, 1u64 << 31, 255) };
            for exponent in [0, 1, bias - 2, bias - 1, bias, bias + 1, bias + 30,
                             bias + 31, bias + 32, bias + 62, bias + 63, bias + 64, max_exp] {
                for fraction in [0, 1, (1u64 << (fraction_bits - 1)) - 1,
                                 1u64 << (fraction_bits - 1), (1u64 << fraction_bits) - 1] {
                    samples.push((exponent << fraction_bits) | fraction);
                    samples.push(sign | (exponent << fraction_bits) | fraction);
                }
            }
            for control in 0..16u64 { for &input in &samples {
                let fpcr = control << 22;
                let native: u64;
                let status: u64;
                // SAFETY: fixed conversion instruction, host FP state restored.
                unsafe { std::arch::asm!(
                    "mrs x8, fpcr", "mrs x9, fpsr",
                    "msr fpcr, {control}", "msr fpsr, {initial}",
                    $load, $op, "mrs {status}, fpsr",
                    "msr fpcr, x8", "msr fpsr, x9", "mov {result}, x10",
                    control = in(reg) fpcr, initial = in(reg) 2u64, input = in(reg) input,
                    status = out(reg) status, result = out(reg) native,
                    out("x8") _, out("x9") _, out("x10") _, out("v0") _,
                    options(nostack, nomem, preserves_flags),
                ); }
                cpu.pc = 0; cpu.v[0] = u128::from(input); cpu.set_x(0, u64::MAX);
                cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 2;
                cpu.step().unwrap();
                assert_eq!((cpu.x(0), cpu.sysregs.fpsr), (native, status),
                    "{} input={input:#x} fpcr={fpcr:#x}", $op);
            }}
        }};
    }
    check!("fcvtzu w10, d0", "fmov d0, {input}", 0x1e79_0000, true);
    check!("fcvtzs w10, s0", "fmov s0, {input:w}", 0x1e380000, false);
    check!("fcvtzs w10, d0", "fmov d0, {input}", 0x1e780000, true);
    check!("fcvtzs x10, s0", "fmov s0, {input:w}", 0x9e380000, false);
    check!("fcvtzs x10, d0", "fmov d0, {input}", 0x9e780000, true);
    check!("fcvtzu w10, s0", "fmov s0, {input:w}", 0x1e390000, false);
    check!("fcvtzu x10, s0", "fmov s0, {input:w}", 0x9e390000, false);
    check!("fcvtzu x10, d0", "fmov d0, {input}", 0x9e790000, true);
    check!("fcvtns w10, s0", "fmov s0, {input:w}", 0x1e200000, false);
    check!("fcvtns w10, d0", "fmov d0, {input}", 0x1e600000, true);
    check!("fcvtns x10, s0", "fmov s0, {input:w}", 0x9e200000, false);
    check!("fcvtns x10, d0", "fmov d0, {input}", 0x9e600000, true);
    check!("fcvtnu w10, s0", "fmov s0, {input:w}", 0x1e210000, false);
    check!("fcvtnu w10, d0", "fmov d0, {input}", 0x1e610000, true);
    check!("fcvtnu x10, s0", "fmov s0, {input:w}", 0x9e210000, false);
    check!("fcvtnu x10, d0", "fmov d0, {input}", 0x9e610000, true);
    check!("fcvtas w10, s0", "fmov s0, {input:w}", 0x1e240000, false);
    check!("fcvtas w10, d0", "fmov d0, {input}", 0x1e640000, true);
    check!("fcvtas x10, s0", "fmov s0, {input:w}", 0x9e240000, false);
    check!("fcvtas x10, d0", "fmov d0, {input}", 0x9e640000, true);
    check!("fcvtau w10, s0", "fmov s0, {input:w}", 0x1e250000, false);
    check!("fcvtau w10, d0", "fmov d0, {input}", 0x1e650000, true);
    check!("fcvtau x10, s0", "fmov s0, {input:w}", 0x9e250000, false);
    check!("fcvtau x10, d0", "fmov d0, {input}", 0x9e650000, true);
    check!("fcvtms w10, s0", "fmov s0, {input:w}", 0x1e300000, false);
    check!("fcvtms w10, d0", "fmov d0, {input}", 0x1e700000, true);
    check!("fcvtms x10, s0", "fmov s0, {input:w}", 0x9e300000, false);
    check!("fcvtms x10, d0", "fmov d0, {input}", 0x9e700000, true);
    check!("fcvtmu w10, s0", "fmov s0, {input:w}", 0x1e310000, false);
    check!("fcvtmu w10, d0", "fmov d0, {input}", 0x1e710000, true);
    check!("fcvtmu x10, s0", "fmov s0, {input:w}", 0x9e310000, false);
    check!("fcvtmu x10, d0", "fmov d0, {input}", 0x9e710000, true);
    check!("fcvtps w10, s0", "fmov s0, {input:w}", 0x1e280000, false);
    check!("fcvtps w10, d0", "fmov d0, {input}", 0x1e680000, true);
    check!("fcvtps x10, s0", "fmov s0, {input:w}", 0x9e280000, false);
    check!("fcvtps x10, d0", "fmov d0, {input}", 0x9e680000, true);
    check!("fcvtpu w10, s0", "fmov s0, {input:w}", 0x1e290000, false);
    check!("fcvtpu w10, d0", "fmov d0, {input}", 0x1e690000, true);
    check!("fcvtpu x10, s0", "fmov s0, {input:w}", 0x9e290000, false);
    check!("fcvtpu x10, d0", "fmov d0, {input}", 0x9e690000, true);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_comparisons_match_native_all_lane_widths() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01] {
                for right in [1u128, u128::MAX, 0x0101_0101_0101_0101_0101_0101_0101_0101] {
                    let mut native = 0u128;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("cmeq d2, d0, #0", 0x5ee09802);
    check!("cmgt d2, d0, #0", 0x5ee08802);
    check!("cmge d2, d0, #0", 0x7ee08802);
    check!("cmle d2, d0, #0", 0x7ee09802);
    check!("cmlt d2, d0, #0", 0x5ee0a802);
    check!("cmeq d2, d0, d1", 0x7ee18c02);
    check!("cmtst d2, d0, d1", 0x5ee18c02);
    check!("cmgt d2, d0, d1", 0x5ee13402);
    check!("cmge d2, d0, d1", 0x5ee13c02);
    check!("cmhi d2, d0, d1", 0x7ee13402);
    check!("cmhs d2, d0, d1", 0x7ee13c02);
    check!("cmeq v2.8b, v0.8b, #0", 0x0e209802);
    check!("cmeq v2.16b, v0.16b, #0", 0x4e209802);
    check!("cmeq v2.4h, v0.4h, #0", 0x0e609802);
    check!("cmeq v2.8h, v0.8h, #0", 0x4e609802);
    check!("cmeq v2.2s, v0.2s, #0", 0x0ea09802);
    check!("cmeq v2.4s, v0.4s, #0", 0x4ea09802);
    check!("cmeq v2.2d, v0.2d, #0", 0x4ee09802);
    check!("cmgt v2.8b, v0.8b, #0", 0x0e208802);
    check!("cmgt v2.16b, v0.16b, #0", 0x4e208802);
    check!("cmgt v2.4h, v0.4h, #0", 0x0e608802);
    check!("cmgt v2.8h, v0.8h, #0", 0x4e608802);
    check!("cmgt v2.2s, v0.2s, #0", 0x0ea08802);
    check!("cmgt v2.4s, v0.4s, #0", 0x4ea08802);
    check!("cmgt v2.2d, v0.2d, #0", 0x4ee08802);
    check!("cmge v2.8b, v0.8b, #0", 0x2e208802);
    check!("cmge v2.16b, v0.16b, #0", 0x6e208802);
    check!("cmge v2.4h, v0.4h, #0", 0x2e608802);
    check!("cmge v2.8h, v0.8h, #0", 0x6e608802);
    check!("cmge v2.2s, v0.2s, #0", 0x2ea08802);
    check!("cmge v2.4s, v0.4s, #0", 0x6ea08802);
    check!("cmge v2.2d, v0.2d, #0", 0x6ee08802);
    check!("cmle v2.8b, v0.8b, #0", 0x2e209802);
    check!("cmle v2.16b, v0.16b, #0", 0x6e209802);
    check!("cmle v2.4h, v0.4h, #0", 0x2e609802);
    check!("cmle v2.8h, v0.8h, #0", 0x6e609802);
    check!("cmle v2.2s, v0.2s, #0", 0x2ea09802);
    check!("cmle v2.4s, v0.4s, #0", 0x6ea09802);
    check!("cmle v2.2d, v0.2d, #0", 0x6ee09802);
    check!("cmlt v2.8b, v0.8b, #0", 0x0e20a802);
    check!("cmlt v2.16b, v0.16b, #0", 0x4e20a802);
    check!("cmlt v2.4h, v0.4h, #0", 0x0e60a802);
    check!("cmlt v2.8h, v0.8h, #0", 0x4e60a802);
    check!("cmlt v2.2s, v0.2s, #0", 0x0ea0a802);
    check!("cmlt v2.4s, v0.4s, #0", 0x4ea0a802);
    check!("cmlt v2.2d, v0.2d, #0", 0x4ee0a802);
    check!("cmeq v2.8b, v0.8b, v1.8b", 0x2e218c02);
    check!("cmeq v2.16b, v0.16b, v1.16b", 0x6e218c02);
    check!("cmeq v2.4h, v0.4h, v1.4h", 0x2e618c02);
    check!("cmeq v2.8h, v0.8h, v1.8h", 0x6e618c02);
    check!("cmeq v2.2s, v0.2s, v1.2s", 0x2ea18c02);
    check!("cmeq v2.4s, v0.4s, v1.4s", 0x6ea18c02);
    check!("cmeq v2.2d, v0.2d, v1.2d", 0x6ee18c02);
    check!("cmtst v2.8b, v0.8b, v1.8b", 0x0e218c02);
    check!("cmtst v2.16b, v0.16b, v1.16b", 0x4e218c02);
    check!("cmtst v2.4h, v0.4h, v1.4h", 0x0e618c02);
    check!("cmtst v2.8h, v0.8h, v1.8h", 0x4e618c02);
    check!("cmtst v2.2s, v0.2s, v1.2s", 0x0ea18c02);
    check!("cmtst v2.4s, v0.4s, v1.4s", 0x4ea18c02);
    check!("cmtst v2.2d, v0.2d, v1.2d", 0x4ee18c02);
    check!("cmgt v2.8b, v0.8b, v1.8b", 0x0e213402);
    check!("cmgt v2.16b, v0.16b, v1.16b", 0x4e213402);
    check!("cmgt v2.4h, v0.4h, v1.4h", 0x0e613402);
    check!("cmgt v2.8h, v0.8h, v1.8h", 0x4e613402);
    check!("cmgt v2.2s, v0.2s, v1.2s", 0x0ea13402);
    check!("cmgt v2.4s, v0.4s, v1.4s", 0x4ea13402);
    check!("cmgt v2.2d, v0.2d, v1.2d", 0x4ee13402);
    check!("cmge v2.8b, v0.8b, v1.8b", 0x0e213c02);
    check!("cmge v2.16b, v0.16b, v1.16b", 0x4e213c02);
    check!("cmge v2.4h, v0.4h, v1.4h", 0x0e613c02);
    check!("cmge v2.8h, v0.8h, v1.8h", 0x4e613c02);
    check!("cmge v2.2s, v0.2s, v1.2s", 0x0ea13c02);
    check!("cmge v2.4s, v0.4s, v1.4s", 0x4ea13c02);
    check!("cmge v2.2d, v0.2d, v1.2d", 0x4ee13c02);
    check!("cmhi v2.8b, v0.8b, v1.8b", 0x2e213402);
    check!("cmhi v2.16b, v0.16b, v1.16b", 0x6e213402);
    check!("cmhi v2.4h, v0.4h, v1.4h", 0x2e613402);
    check!("cmhi v2.8h, v0.8h, v1.8h", 0x6e613402);
    check!("cmhi v2.2s, v0.2s, v1.2s", 0x2ea13402);
    check!("cmhi v2.4s, v0.4s, v1.4s", 0x6ea13402);
    check!("cmhi v2.2d, v0.2d, v1.2d", 0x6ee13402);
    check!("cmhs v2.8b, v0.8b, v1.8b", 0x2e213c02);
    check!("cmhs v2.16b, v0.16b, v1.16b", 0x6e213c02);
    check!("cmhs v2.4h, v0.4h, v1.4h", 0x2e613c02);
    check!("cmhs v2.8h, v0.8h, v1.8h", 0x6e613c02);
    check!("cmhs v2.2s, v0.2s, v1.2s", 0x2ea13c02);
    check!("cmhs v2.4s, v0.4s, v1.4s", 0x6ea13c02);
    check!("cmhs v2.2d, v0.2d, v1.2d", 0x6ee13c02);
}

#[test]
fn reserved_simd_arithmetic_encodings_fail_without_mutation() {
    for instruction in [
        0x1ee1_8402,
        0x5e21_8402,
        0x0ee1_8402,
        0x2ee1_8402,
        0x0ee1_8c02,
        0x0ee0_9802,
        0x5e20_9802, // Scalar integer comparisons require a D lane.
        0x5ea0_9802,
        0x5ea1_8c02,
        0x0e08_0c02,
        0x2f00_f400, // Reserved Q=0 vector-double immediate.
        0x0f40_a401, // Widening a 64-bit element is unavailable.
        0x0ee1_2802, // Reserved XTN size.
        0x0ee1_0002, // Widening arithmetic cannot widen a D lane.
        0x6ee1_3002,
        0x4c41_a020, // Non-post-index structure transfer requires Rm=0.
        0x4c40_f020, // Unallocated multiple-structure opcode.
        0x0c40_8c20, // LD2..4 cannot use Q=0 D elements.
        0x0c40_4c20,
        0x0c40_0c20,
        0x0f40_5402, // Q=0 vector immediate shifts cannot use D lanes.
        0x5f20_5402, // Scalar immediate shifts require D lanes.
        0x1f40_5402, // Scalar encoding requires Q=1.
        0x4f40_4402, // SRI requires U=1.
        0x0d40_4420, // Halfword lane transfer requires size[0]=0.
        0x0d40_8820, // Word/doubleword transfer requires size[1]=0.
        0x0d40_9420, // Doubleword lane transfer requires S=0.
        0x0d40_d020, // Replication requires S=0.
        0x0d00_c020, // Store-and-replicate encoding is unallocated.
        0x5e00_0402, // Scalar DUP requires a nonzero width/index immediate.
        0x5e10_0402, // Scalar DUP cannot extract a 128-bit element.
        0x2e01_4002, // 64-bit EXT offset must be below eight.
        0x0ec1_1802, // Q=0 UZP with 64-bit lanes.
        0x4e00_1c00, // INS cannot have imm5=0.
        0x4e10_1c00, // INS cannot insert a 128-bit lane.
        0x6e00_0420, // Vector INS cannot have imm5=0.
    ] {
        let mut cpu = cpu_for(instruction);
        cpu.v[0] = u128::MAX;
        cpu.v[1] = 7;
        cpu.v[2] = 123;
        cpu.nzcv = 0xa;
        cpu.sysregs.fpsr = 0x0800_009f;
        let vectors = cpu.v;
        assert!(cpu.step().is_err(), "reserved {instruction:#x} accepted");
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.v, vectors);
        assert_eq!(cpu.nzcv, 0xa);
        assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn pairwise_integer_reductions_match_native() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01] {
                for right in [1u128, u128::MAX, 0x0101_0101_0101_0101_0101_0101_0101_0101] {
                    let mut native = 0u128;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("umaxp v2.8b, v0.8b, v1.8b", 0x2e21a402);
    check!("umaxp v2.16b, v0.16b, v1.16b", 0x6e21a402);
    check!("umaxp v2.4h, v0.4h, v1.4h", 0x2e61a402);
    check!("umaxp v2.8h, v0.8h, v1.8h", 0x6e61a402);
    check!("umaxp v2.2s, v0.2s, v1.2s", 0x2ea1a402);
    check!("umaxp v2.4s, v0.4s, v1.4s", 0x6ea1a402);
    check!("uminp v2.8b, v0.8b, v1.8b", 0x2e21ac02);
    check!("uminp v2.16b, v0.16b, v1.16b", 0x6e21ac02);
    check!("uminp v2.4h, v0.4h, v1.4h", 0x2e61ac02);
    check!("uminp v2.8h, v0.8h, v1.8h", 0x6e61ac02);
    check!("uminp v2.2s, v0.2s, v1.2s", 0x2ea1ac02);
    check!("uminp v2.4s, v0.4s, v1.4s", 0x6ea1ac02);
    check!("smaxp v2.8b, v0.8b, v1.8b", 0x0e21a402);
    check!("smaxp v2.16b, v0.16b, v1.16b", 0x4e21a402);
    check!("smaxp v2.4h, v0.4h, v1.4h", 0x0e61a402);
    check!("smaxp v2.8h, v0.8h, v1.8h", 0x4e61a402);
    check!("smaxp v2.2s, v0.2s, v1.2s", 0x0ea1a402);
    check!("smaxp v2.4s, v0.4s, v1.4s", 0x4ea1a402);
    check!("sminp v2.8b, v0.8b, v1.8b", 0x0e21ac02);
    check!("sminp v2.16b, v0.16b, v1.16b", 0x4e21ac02);
    check!("sminp v2.4h, v0.4h, v1.4h", 0x0e61ac02);
    check!("sminp v2.8h, v0.8h, v1.8h", 0x4e61ac02);
    check!("sminp v2.2s, v0.2s, v1.2s", 0x0ea1ac02);
    check!("sminp v2.4s, v0.4s, v1.4s", 0x4ea1ac02);
    check!("addp v2.8b, v0.8b, v1.8b", 0x0e21bc02);
    check!("addp v2.16b, v0.16b, v1.16b", 0x4e21bc02);
    check!("addp v2.4h, v0.4h, v1.4h", 0x0e61bc02);
    check!("addp v2.8h, v0.8h, v1.8h", 0x4e61bc02);
    check!("addp v2.2s, v0.2s, v1.2s", 0x0ea1bc02);
    check!("addp v2.4s, v0.4s, v1.4s", 0x4ea1bc02);
    check!("addp v2.2d, v0.2d, v1.2d", 0x4ee1bc02);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_bitwise_operations_match_native() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01] {
                for right in [1u128, u128::MAX, 0x0101_0101_0101_0101_0101_0101_0101_0101] {
                    let initial = left.rotate_left(37) ^ right;
                    let mut native = initial;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q2, [{output}]", "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = initial;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("orr v2.8b, v0.8b, v1.8b", 0x0ea11c02);
    check!("orr v2.16b, v0.16b, v1.16b", 0x4ea11c02);
    check!("orn v2.8b, v0.8b, v1.8b", 0x0ee11c02);
    check!("orn v2.16b, v0.16b, v1.16b", 0x4ee11c02);
    check!("and v2.8b, v0.8b, v1.8b", 0x0e211c02);
    check!("and v2.16b, v0.16b, v1.16b", 0x4e211c02);
    check!("bic v2.8b, v0.8b, v1.8b", 0x0e611c02);
    check!("bic v2.16b, v0.16b, v1.16b", 0x4e611c02);
    check!("eor v2.8b, v0.8b, v1.8b", 0x2e211c02);
    check!("eor v2.16b, v0.16b, v1.16b", 0x6e211c02);
    check!("bsl v2.8b, v0.8b, v1.8b", 0x2e611c02);
    check!("bsl v2.16b, v0.16b, v1.16b", 0x6e611c02);
    check!("bit v2.8b, v0.8b, v1.8b", 0x2ea11c02);
    check!("bit v2.16b, v0.16b, v1.16b", 0x6ea11c02);
    check!("bif v2.8b, v0.8b, v1.8b", 0x2ee11c02);
    check!("bif v2.16b, v0.16b, v1.16b", 0x6ee11c02);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn modified_immediates_match_native_every_encoding() {
    #[inline(never)]
    fn check<const BASE: u32>() {
        let mut cpu = cpu_for(BASE);
        for seed in [0u128, u128::MAX, 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210] {
            let mut native = [0u128; 256];
            // SAFETY: assembler expands 256 fixed instruction encodings at
            // compile time. Writes exactly 256 initialized vector slots.
            // Guest bytes never become executable code.
            unsafe {
                std::arch::asm!(
                    ".set .Lrelay_modified_imm, 0", ".rept 256",
                    "ldr q0, [{seed}]",
                    ".inst {base} + ((.Lrelay_modified_imm & 224) << 11) + ((.Lrelay_modified_imm & 31) << 5)",
                    "str q0, [{output}], #16",
                    ".set .Lrelay_modified_imm, .Lrelay_modified_imm + 1", ".endr",
                    base = const BASE, seed = in(reg) &seed,
                    output = inout(reg) native.as_mut_ptr() => _,
                    out("v0") _, options(nostack, preserves_flags),
                );
            }
            for (immediate, expected) in native.into_iter().enumerate() {
                let word: u32 =
                    BASE | (((immediate as u32) & 224) << 11) | (((immediate as u32) & 31) << 5);
                cpu.memory.write(0, &word.to_le_bytes()).unwrap();
                cpu.pc = 0;
                cpu.v[0] = seed;
                cpu.step().unwrap();
                assert_eq!(cpu.v[0], expected, "word={word:#010x} seed={seed:#034x}");
            }
        }
    }
    check::<0x0f000400>();
    check::<0x0f001400>();
    check::<0x0f002400>();
    check::<0x0f003400>();
    check::<0x0f004400>();
    check::<0x0f005400>();
    check::<0x0f006400>();
    check::<0x0f007400>();
    check::<0x0f008400>();
    check::<0x0f009400>();
    check::<0x0f00a400>();
    check::<0x0f00b400>();
    check::<0x0f00c400>();
    check::<0x0f00d400>();
    check::<0x0f00e400>();
    check::<0x0f00f400>();
    check::<0x4f000400>();
    check::<0x4f001400>();
    check::<0x4f002400>();
    check::<0x4f003400>();
    check::<0x4f004400>();
    check::<0x4f005400>();
    check::<0x4f006400>();
    check::<0x4f007400>();
    check::<0x4f008400>();
    check::<0x4f009400>();
    check::<0x4f00a400>();
    check::<0x4f00b400>();
    check::<0x4f00c400>();
    check::<0x4f00d400>();
    check::<0x4f00e400>();
    check::<0x4f00f400>();
    check::<0x2f000400>();
    check::<0x2f001400>();
    check::<0x2f002400>();
    check::<0x2f003400>();
    check::<0x2f004400>();
    check::<0x2f005400>();
    check::<0x2f006400>();
    check::<0x2f007400>();
    check::<0x2f008400>();
    check::<0x2f009400>();
    check::<0x2f00a400>();
    check::<0x2f00b400>();
    check::<0x2f00c400>();
    check::<0x2f00d400>();
    check::<0x2f00e400>();
    check::<0x6f000400>();
    check::<0x6f001400>();
    check::<0x6f002400>();
    check::<0x6f003400>();
    check::<0x6f004400>();
    check::<0x6f005400>();
    check::<0x6f006400>();
    check::<0x6f007400>();
    check::<0x6f008400>();
    check::<0x6f009400>();
    check::<0x6f00a400>();
    check::<0x6f00b400>();
    check::<0x6f00c400>();
    check::<0x6f00d400>();
    check::<0x6f00e400>();
    check::<0x6f00f400>();
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn widening_shifts_match_native_every_legal_shift() {
    #[inline(never)]
    fn check<const BASE: u32, const BITS: usize>() {
        let mut cpu = cpu_for(BASE);
        for source in [0u128, u128::MAX, 0x8000_7fff_ffff_0001_0180_7fff_80ff_01ff] {
            let mut native = [0u128; 32];
            // SAFETY: fixed compile-time instructions, at most 32 vector writes.
            unsafe {
                std::arch::asm!(
                    "ldr q0, [{source}]", ".set .Lrelay_widen_shift, 0", ".rept {count}",
                    ".inst {base} + (.Lrelay_widen_shift << 16)", "str q1, [{output}], #16",
                    ".set .Lrelay_widen_shift, .Lrelay_widen_shift + 1", ".endr",
                    base = const BASE, count = const BITS, source = in(reg) &source,
                    output = inout(reg) native.as_mut_ptr() => _, out("v0") _, out("v1") _,
                    options(nostack, preserves_flags),
                );
            }
            for (shift, expected) in native.into_iter().take(BITS).enumerate() {
                let word = BASE | ((shift as u32) << 16);
                cpu.memory.write(0, &word.to_le_bytes()).unwrap();
                cpu.pc = 0;
                cpu.v[0] = source;
                cpu.v[1] = u128::MAX;
                cpu.step().unwrap();
                assert_eq!(cpu.v[1], expected, "word={word:#x} source={source:#x}");
            }
        }
    }
    check::<0x0f08_a401, 8>();
    check::<0x0f10_a401, 16>();
    check::<0x0f20_a401, 32>();
    check::<0x2f08_a401, 8>();
    check::<0x2f10_a401, 16>();
    check::<0x2f20_a401, 32>();
    check::<0x4f08_a401, 8>();
    check::<0x4f10_a401, 16>();
    check::<0x4f20_a401, 32>();
    check::<0x6f08_a401, 8>();
    check::<0x6f10_a401, 16>();
    check::<0x6f20_a401, 32>();
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_byte_unary_operations_match_native() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01] {
                for right in [1u128, u128::MAX, 0x0101_0101_0101_0101_0101_0101_0101_0101] {
                    let mut native = 0u128;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("mvn v2.8b, v0.8b", 0x2e205802);
    check!("mvn v2.16b, v0.16b", 0x6e205802);
    check!("cnt v2.8b, v0.8b", 0x0e205802);
    check!("cnt v2.16b, v0.16b", 0x4e205802);
    check!("rbit v2.8b, v0.8b", 0x2e605802);
    check!("rbit v2.16b, v0.16b", 0x6e605802);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_extract_matches_native_all_offsets() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01] {
                for right in [1u128, u128::MAX, 0x0101_0101_0101_0101_0101_0101_0101_0101] {
                    let mut native = 0u128;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("ext v2.8b, v0.8b, v1.8b, #0", 0x2e010002);
    check!("ext v2.8b, v0.8b, v1.8b, #1", 0x2e010802);
    check!("ext v2.8b, v0.8b, v1.8b, #2", 0x2e011002);
    check!("ext v2.8b, v0.8b, v1.8b, #3", 0x2e011802);
    check!("ext v2.8b, v0.8b, v1.8b, #4", 0x2e012002);
    check!("ext v2.8b, v0.8b, v1.8b, #5", 0x2e012802);
    check!("ext v2.8b, v0.8b, v1.8b, #6", 0x2e013002);
    check!("ext v2.8b, v0.8b, v1.8b, #7", 0x2e013802);
    check!("ext v2.16b, v0.16b, v1.16b, #0", 0x6e010002);
    check!("ext v2.16b, v0.16b, v1.16b, #1", 0x6e010802);
    check!("ext v2.16b, v0.16b, v1.16b, #2", 0x6e011002);
    check!("ext v2.16b, v0.16b, v1.16b, #3", 0x6e011802);
    check!("ext v2.16b, v0.16b, v1.16b, #4", 0x6e012002);
    check!("ext v2.16b, v0.16b, v1.16b, #5", 0x6e012802);
    check!("ext v2.16b, v0.16b, v1.16b, #6", 0x6e013002);
    check!("ext v2.16b, v0.16b, v1.16b, #7", 0x6e013802);
    check!("ext v2.16b, v0.16b, v1.16b, #8", 0x6e014002);
    check!("ext v2.16b, v0.16b, v1.16b, #9", 0x6e014802);
    check!("ext v2.16b, v0.16b, v1.16b, #10", 0x6e015002);
    check!("ext v2.16b, v0.16b, v1.16b, #11", 0x6e015802);
    check!("ext v2.16b, v0.16b, v1.16b, #12", 0x6e016002);
    check!("ext v2.16b, v0.16b, v1.16b, #13", 0x6e016802);
    check!("ext v2.16b, v0.16b, v1.16b, #14", 0x6e017002);
    check!("ext v2.16b, v0.16b, v1.16b, #15", 0x6e017802);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn scalar_float_immediates_match_native_every_encoding() {
    #[inline(never)]
    fn check<const BASE: u32>() {
        let mut cpu = cpu_for(BASE);
        for seed in [0u128, u128::MAX, 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210] {
            let mut native = [0u128; 256];
            // SAFETY: assembler expands 256 fixed instruction encodings at
            // compile time. Writes exactly 256 initialized vector slots.
            // Guest bytes never become executable code.
            unsafe {
                std::arch::asm!(
                    ".set .Lrelay_modified_imm, 0", ".rept 256",
                    "ldr q0, [{seed}]",
                    ".inst {base} + (.Lrelay_modified_imm << 13)",
                    "str q0, [{output}], #16",
                    ".set .Lrelay_modified_imm, .Lrelay_modified_imm + 1", ".endr",
                    base = const BASE, seed = in(reg) &seed,
                    output = inout(reg) native.as_mut_ptr() => _,
                    out("v0") _, options(nostack, preserves_flags),
                );
            }
            for (immediate, expected) in native.into_iter().enumerate() {
                let word: u32 = BASE | ((immediate as u32) << 13);
                cpu.memory.write(0, &word.to_le_bytes()).unwrap();
                cpu.pc = 0;
                cpu.v[0] = seed;
                cpu.step().unwrap();
                assert_eq!(cpu.v[0], expected, "word={word:#010x} seed={seed:#034x}");
            }
        }
    }
    check::<0x1e20_1000>();
    check::<0x1e60_1000>();
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn lane_insertions_match_native_all_source_and_destination_lanes() {
    #[inline(never)]
    fn check<const WORD: u32>() {
        let mut cpu = cpu_for(WORD);
        for seed in [0u128, u128::MAX, 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210] {
            for source in [0u128, u128::MAX, 0x8001_7ffe_0080_ff7f_0123_4567_89ab_cdef] {
                let mut native = seed;
                // SAFETY: all lane encodings are fixed at compile time; valid
                // input/output buffers, no guest code or executable allocation.
                unsafe {
                    std::arch::asm!(
                        "ldr q0, [{output}]", "ldr q1, [{source}]", ".inst {word}",
                        "str q0, [{output}]", word = const WORD,
                        output = in(reg) &mut native, source = in(reg) &source,
                        in("x0") source as u64, out("v0") _, out("v1") _,
                        options(nostack, preserves_flags),
                    );
                }
                cpu.pc = 0;
                cpu.v[0] = seed;
                cpu.v[1] = source;
                cpu.set_x(0, source as u64);
                cpu.step().unwrap();
                assert_eq!(
                    cpu.v[0], native,
                    "word={WORD:#x} seed={seed:#x} source={source:#x}"
                );
                assert_eq!(cpu.v[1], source);
            }
        }
    }
    check::<0x4e011c00>();
    check::<0x6e010420>();
    check::<0x6e010c20>();
    check::<0x6e011420>();
    check::<0x6e011c20>();
    check::<0x6e012420>();
    check::<0x6e012c20>();
    check::<0x6e013420>();
    check::<0x6e013c20>();
    check::<0x6e014420>();
    check::<0x6e014c20>();
    check::<0x6e015420>();
    check::<0x6e015c20>();
    check::<0x6e016420>();
    check::<0x6e016c20>();
    check::<0x6e017420>();
    check::<0x6e017c20>();
    check::<0x4e031c00>();
    check::<0x6e030420>();
    check::<0x6e030c20>();
    check::<0x6e031420>();
    check::<0x6e031c20>();
    check::<0x6e032420>();
    check::<0x6e032c20>();
    check::<0x6e033420>();
    check::<0x6e033c20>();
    check::<0x6e034420>();
    check::<0x6e034c20>();
    check::<0x6e035420>();
    check::<0x6e035c20>();
    check::<0x6e036420>();
    check::<0x6e036c20>();
    check::<0x6e037420>();
    check::<0x6e037c20>();
    check::<0x4e051c00>();
    check::<0x6e050420>();
    check::<0x6e050c20>();
    check::<0x6e051420>();
    check::<0x6e051c20>();
    check::<0x6e052420>();
    check::<0x6e052c20>();
    check::<0x6e053420>();
    check::<0x6e053c20>();
    check::<0x6e054420>();
    check::<0x6e054c20>();
    check::<0x6e055420>();
    check::<0x6e055c20>();
    check::<0x6e056420>();
    check::<0x6e056c20>();
    check::<0x6e057420>();
    check::<0x6e057c20>();
    check::<0x4e071c00>();
    check::<0x6e070420>();
    check::<0x6e070c20>();
    check::<0x6e071420>();
    check::<0x6e071c20>();
    check::<0x6e072420>();
    check::<0x6e072c20>();
    check::<0x6e073420>();
    check::<0x6e073c20>();
    check::<0x6e074420>();
    check::<0x6e074c20>();
    check::<0x6e075420>();
    check::<0x6e075c20>();
    check::<0x6e076420>();
    check::<0x6e076c20>();
    check::<0x6e077420>();
    check::<0x6e077c20>();
    check::<0x4e091c00>();
    check::<0x6e090420>();
    check::<0x6e090c20>();
    check::<0x6e091420>();
    check::<0x6e091c20>();
    check::<0x6e092420>();
    check::<0x6e092c20>();
    check::<0x6e093420>();
    check::<0x6e093c20>();
    check::<0x6e094420>();
    check::<0x6e094c20>();
    check::<0x6e095420>();
    check::<0x6e095c20>();
    check::<0x6e096420>();
    check::<0x6e096c20>();
    check::<0x6e097420>();
    check::<0x6e097c20>();
    check::<0x4e0b1c00>();
    check::<0x6e0b0420>();
    check::<0x6e0b0c20>();
    check::<0x6e0b1420>();
    check::<0x6e0b1c20>();
    check::<0x6e0b2420>();
    check::<0x6e0b2c20>();
    check::<0x6e0b3420>();
    check::<0x6e0b3c20>();
    check::<0x6e0b4420>();
    check::<0x6e0b4c20>();
    check::<0x6e0b5420>();
    check::<0x6e0b5c20>();
    check::<0x6e0b6420>();
    check::<0x6e0b6c20>();
    check::<0x6e0b7420>();
    check::<0x6e0b7c20>();
    check::<0x4e0d1c00>();
    check::<0x6e0d0420>();
    check::<0x6e0d0c20>();
    check::<0x6e0d1420>();
    check::<0x6e0d1c20>();
    check::<0x6e0d2420>();
    check::<0x6e0d2c20>();
    check::<0x6e0d3420>();
    check::<0x6e0d3c20>();
    check::<0x6e0d4420>();
    check::<0x6e0d4c20>();
    check::<0x6e0d5420>();
    check::<0x6e0d5c20>();
    check::<0x6e0d6420>();
    check::<0x6e0d6c20>();
    check::<0x6e0d7420>();
    check::<0x6e0d7c20>();
    check::<0x4e0f1c00>();
    check::<0x6e0f0420>();
    check::<0x6e0f0c20>();
    check::<0x6e0f1420>();
    check::<0x6e0f1c20>();
    check::<0x6e0f2420>();
    check::<0x6e0f2c20>();
    check::<0x6e0f3420>();
    check::<0x6e0f3c20>();
    check::<0x6e0f4420>();
    check::<0x6e0f4c20>();
    check::<0x6e0f5420>();
    check::<0x6e0f5c20>();
    check::<0x6e0f6420>();
    check::<0x6e0f6c20>();
    check::<0x6e0f7420>();
    check::<0x6e0f7c20>();
    check::<0x4e111c00>();
    check::<0x6e110420>();
    check::<0x6e110c20>();
    check::<0x6e111420>();
    check::<0x6e111c20>();
    check::<0x6e112420>();
    check::<0x6e112c20>();
    check::<0x6e113420>();
    check::<0x6e113c20>();
    check::<0x6e114420>();
    check::<0x6e114c20>();
    check::<0x6e115420>();
    check::<0x6e115c20>();
    check::<0x6e116420>();
    check::<0x6e116c20>();
    check::<0x6e117420>();
    check::<0x6e117c20>();
    check::<0x4e131c00>();
    check::<0x6e130420>();
    check::<0x6e130c20>();
    check::<0x6e131420>();
    check::<0x6e131c20>();
    check::<0x6e132420>();
    check::<0x6e132c20>();
    check::<0x6e133420>();
    check::<0x6e133c20>();
    check::<0x6e134420>();
    check::<0x6e134c20>();
    check::<0x6e135420>();
    check::<0x6e135c20>();
    check::<0x6e136420>();
    check::<0x6e136c20>();
    check::<0x6e137420>();
    check::<0x6e137c20>();
    check::<0x4e151c00>();
    check::<0x6e150420>();
    check::<0x6e150c20>();
    check::<0x6e151420>();
    check::<0x6e151c20>();
    check::<0x6e152420>();
    check::<0x6e152c20>();
    check::<0x6e153420>();
    check::<0x6e153c20>();
    check::<0x6e154420>();
    check::<0x6e154c20>();
    check::<0x6e155420>();
    check::<0x6e155c20>();
    check::<0x6e156420>();
    check::<0x6e156c20>();
    check::<0x6e157420>();
    check::<0x6e157c20>();
    check::<0x4e171c00>();
    check::<0x6e170420>();
    check::<0x6e170c20>();
    check::<0x6e171420>();
    check::<0x6e171c20>();
    check::<0x6e172420>();
    check::<0x6e172c20>();
    check::<0x6e173420>();
    check::<0x6e173c20>();
    check::<0x6e174420>();
    check::<0x6e174c20>();
    check::<0x6e175420>();
    check::<0x6e175c20>();
    check::<0x6e176420>();
    check::<0x6e176c20>();
    check::<0x6e177420>();
    check::<0x6e177c20>();
    check::<0x4e191c00>();
    check::<0x6e190420>();
    check::<0x6e190c20>();
    check::<0x6e191420>();
    check::<0x6e191c20>();
    check::<0x6e192420>();
    check::<0x6e192c20>();
    check::<0x6e193420>();
    check::<0x6e193c20>();
    check::<0x6e194420>();
    check::<0x6e194c20>();
    check::<0x6e195420>();
    check::<0x6e195c20>();
    check::<0x6e196420>();
    check::<0x6e196c20>();
    check::<0x6e197420>();
    check::<0x6e197c20>();
    check::<0x4e1b1c00>();
    check::<0x6e1b0420>();
    check::<0x6e1b0c20>();
    check::<0x6e1b1420>();
    check::<0x6e1b1c20>();
    check::<0x6e1b2420>();
    check::<0x6e1b2c20>();
    check::<0x6e1b3420>();
    check::<0x6e1b3c20>();
    check::<0x6e1b4420>();
    check::<0x6e1b4c20>();
    check::<0x6e1b5420>();
    check::<0x6e1b5c20>();
    check::<0x6e1b6420>();
    check::<0x6e1b6c20>();
    check::<0x6e1b7420>();
    check::<0x6e1b7c20>();
    check::<0x4e1d1c00>();
    check::<0x6e1d0420>();
    check::<0x6e1d0c20>();
    check::<0x6e1d1420>();
    check::<0x6e1d1c20>();
    check::<0x6e1d2420>();
    check::<0x6e1d2c20>();
    check::<0x6e1d3420>();
    check::<0x6e1d3c20>();
    check::<0x6e1d4420>();
    check::<0x6e1d4c20>();
    check::<0x6e1d5420>();
    check::<0x6e1d5c20>();
    check::<0x6e1d6420>();
    check::<0x6e1d6c20>();
    check::<0x6e1d7420>();
    check::<0x6e1d7c20>();
    check::<0x4e1f1c00>();
    check::<0x6e1f0420>();
    check::<0x6e1f0c20>();
    check::<0x6e1f1420>();
    check::<0x6e1f1c20>();
    check::<0x6e1f2420>();
    check::<0x6e1f2c20>();
    check::<0x6e1f3420>();
    check::<0x6e1f3c20>();
    check::<0x6e1f4420>();
    check::<0x6e1f4c20>();
    check::<0x6e1f5420>();
    check::<0x6e1f5c20>();
    check::<0x6e1f6420>();
    check::<0x6e1f6c20>();
    check::<0x6e1f7420>();
    check::<0x6e1f7c20>();
    check::<0x4e021c00>();
    check::<0x6e020420>();
    check::<0x6e021420>();
    check::<0x6e022420>();
    check::<0x6e023420>();
    check::<0x6e024420>();
    check::<0x6e025420>();
    check::<0x6e026420>();
    check::<0x6e027420>();
    check::<0x4e061c00>();
    check::<0x6e060420>();
    check::<0x6e061420>();
    check::<0x6e062420>();
    check::<0x6e063420>();
    check::<0x6e064420>();
    check::<0x6e065420>();
    check::<0x6e066420>();
    check::<0x6e067420>();
    check::<0x4e0a1c00>();
    check::<0x6e0a0420>();
    check::<0x6e0a1420>();
    check::<0x6e0a2420>();
    check::<0x6e0a3420>();
    check::<0x6e0a4420>();
    check::<0x6e0a5420>();
    check::<0x6e0a6420>();
    check::<0x6e0a7420>();
    check::<0x4e0e1c00>();
    check::<0x6e0e0420>();
    check::<0x6e0e1420>();
    check::<0x6e0e2420>();
    check::<0x6e0e3420>();
    check::<0x6e0e4420>();
    check::<0x6e0e5420>();
    check::<0x6e0e6420>();
    check::<0x6e0e7420>();
    check::<0x4e121c00>();
    check::<0x6e120420>();
    check::<0x6e121420>();
    check::<0x6e122420>();
    check::<0x6e123420>();
    check::<0x6e124420>();
    check::<0x6e125420>();
    check::<0x6e126420>();
    check::<0x6e127420>();
    check::<0x4e161c00>();
    check::<0x6e160420>();
    check::<0x6e161420>();
    check::<0x6e162420>();
    check::<0x6e163420>();
    check::<0x6e164420>();
    check::<0x6e165420>();
    check::<0x6e166420>();
    check::<0x6e167420>();
    check::<0x4e1a1c00>();
    check::<0x6e1a0420>();
    check::<0x6e1a1420>();
    check::<0x6e1a2420>();
    check::<0x6e1a3420>();
    check::<0x6e1a4420>();
    check::<0x6e1a5420>();
    check::<0x6e1a6420>();
    check::<0x6e1a7420>();
    check::<0x4e1e1c00>();
    check::<0x6e1e0420>();
    check::<0x6e1e1420>();
    check::<0x6e1e2420>();
    check::<0x6e1e3420>();
    check::<0x6e1e4420>();
    check::<0x6e1e5420>();
    check::<0x6e1e6420>();
    check::<0x6e1e7420>();
    check::<0x4e041c00>();
    check::<0x6e040420>();
    check::<0x6e042420>();
    check::<0x6e044420>();
    check::<0x6e046420>();
    check::<0x4e0c1c00>();
    check::<0x6e0c0420>();
    check::<0x6e0c2420>();
    check::<0x6e0c4420>();
    check::<0x6e0c6420>();
    check::<0x4e141c00>();
    check::<0x6e140420>();
    check::<0x6e142420>();
    check::<0x6e144420>();
    check::<0x6e146420>();
    check::<0x4e1c1c00>();
    check::<0x6e1c0420>();
    check::<0x6e1c2420>();
    check::<0x6e1c4420>();
    check::<0x6e1c6420>();
    check::<0x4e081c00>();
    check::<0x6e080420>();
    check::<0x6e084420>();
    check::<0x4e181c00>();
    check::<0x6e180420>();
    check::<0x6e184420>();
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_narrowing_matches_native_preserved_halves() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01] {
                for right in [1u128, u128::MAX, 0x0101_0101_0101_0101_0101_0101_0101_0101] {
                    let initial = left.rotate_left(37) ^ right;
                    let mut native = initial;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q2, [{output}]", "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = initial;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("xtn v2.8b, v0.8h", 0x0e212802);
    check!("xtn2 v2.16b, v0.8h", 0x4e212802);
    check!("xtn v2.4h, v0.4s", 0x0e612802);
    check!("xtn2 v2.8h, v0.4s", 0x4e612802);
    check!("xtn v2.2s, v0.2d", 0x0ea12802);
    check!("xtn2 v2.4s, v0.2d", 0x4ea12802);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_permutations_match_native_all_widths() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01] {
                for right in [1u128, u128::MAX, 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff] {
                    let mut native = 0u128;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("uzp1 v2.8b, v0.8b, v1.8b", 0x0e011802);
    check!("uzp1 v2.16b, v0.16b, v1.16b", 0x4e011802);
    check!("uzp1 v2.4h, v0.4h, v1.4h", 0x0e411802);
    check!("uzp1 v2.8h, v0.8h, v1.8h", 0x4e411802);
    check!("uzp1 v2.2s, v0.2s, v1.2s", 0x0e811802);
    check!("uzp1 v2.4s, v0.4s, v1.4s", 0x4e811802);
    check!("uzp1 v2.2d, v0.2d, v1.2d", 0x4ec11802);
    check!("uzp2 v2.8b, v0.8b, v1.8b", 0x0e015802);
    check!("uzp2 v2.16b, v0.16b, v1.16b", 0x4e015802);
    check!("uzp2 v2.4h, v0.4h, v1.4h", 0x0e415802);
    check!("uzp2 v2.8h, v0.8h, v1.8h", 0x4e415802);
    check!("uzp2 v2.2s, v0.2s, v1.2s", 0x0e815802);
    check!("uzp2 v2.4s, v0.4s, v1.4s", 0x4e815802);
    check!("uzp2 v2.2d, v0.2d, v1.2d", 0x4ec15802);
    check!("trn1 v2.8b, v0.8b, v1.8b", 0x0e012802);
    check!("trn1 v2.16b, v0.16b, v1.16b", 0x4e012802);
    check!("trn1 v2.4h, v0.4h, v1.4h", 0x0e412802);
    check!("trn1 v2.8h, v0.8h, v1.8h", 0x4e412802);
    check!("trn1 v2.2s, v0.2s, v1.2s", 0x0e812802);
    check!("trn1 v2.4s, v0.4s, v1.4s", 0x4e812802);
    check!("trn1 v2.2d, v0.2d, v1.2d", 0x4ec12802);
    check!("trn2 v2.8b, v0.8b, v1.8b", 0x0e016802);
    check!("trn2 v2.16b, v0.16b, v1.16b", 0x4e016802);
    check!("trn2 v2.4h, v0.4h, v1.4h", 0x0e416802);
    check!("trn2 v2.8h, v0.8h, v1.8h", 0x4e416802);
    check!("trn2 v2.2s, v0.2s, v1.2s", 0x0e816802);
    check!("trn2 v2.4s, v0.4s, v1.4s", 0x4e816802);
    check!("trn2 v2.2d, v0.2d, v1.2d", 0x4ec16802);
    check!("zip1 v2.8b, v0.8b, v1.8b", 0x0e013802);
    check!("zip1 v2.16b, v0.16b, v1.16b", 0x4e013802);
    check!("zip1 v2.4h, v0.4h, v1.4h", 0x0e413802);
    check!("zip1 v2.8h, v0.8h, v1.8h", 0x4e413802);
    check!("zip1 v2.2s, v0.2s, v1.2s", 0x0e813802);
    check!("zip1 v2.4s, v0.4s, v1.4s", 0x4e813802);
    check!("zip1 v2.2d, v0.2d, v1.2d", 0x4ec13802);
    check!("zip2 v2.8b, v0.8b, v1.8b", 0x0e017802);
    check!("zip2 v2.16b, v0.16b, v1.16b", 0x4e017802);
    check!("zip2 v2.4h, v0.4h, v1.4h", 0x0e417802);
    check!("zip2 v2.8h, v0.8h, v1.8h", 0x4e417802);
    check!("zip2 v2.2s, v0.2s, v1.2s", 0x0e817802);
    check!("zip2 v2.4s, v0.4s, v1.4s", 0x4e817802);
    check!("zip2 v2.2d, v0.2d, v1.2d", 0x4ec17802);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn simd_integer_to_float_matches_native_all_forms() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {{
            let mut cpu = cpu_for($word);
            for source in [0u128, u128::MAX, 0x8000_0001_7fff_ffff_8000_0000_0100_0001,
                           0x1234_5678_9abc_def0_fedc_ba98_7654_3210] {
                for control in 0..16u64 {
                    let fpcr = control << 22;
                    let mut native = 0u128;
                    let status: u64;
                    // SAFETY: fixed conversion, valid vector buffers, host FP restored.
                    unsafe { std::arch::asm!(
                        "mrs x8, fpcr", "mrs x9, fpsr", "msr fpcr, {control}", "msr fpsr, {initial}",
                        "ldr q0, [{source}]", $instruction, "str q1, [{output}]",
                        "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                        control = in(reg) fpcr, initial = in(reg) 2u64,
                        source = in(reg) &source, output = in(reg) &mut native,
                        status = out(reg) status, out("x8") _, out("x9") _, out("v0") _, out("v1") _,
                        options(nostack, preserves_flags),
                    ); }
                    cpu.pc = 0; cpu.v[0] = source; cpu.v[1] = u128::MAX;
                    cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 2;
                    cpu.step().unwrap();
                    assert_eq!((cpu.v[1], cpu.sysregs.fpsr), (native, status),
                        "{} source={source:#x} fpcr={fpcr:#x}", $instruction);
                }
            }
        }};
    }
    check!("scvtf d1, d0", 0x5e61_d801);
    check!("scvtf s1, s0", 0x5e21_d801);
    check!("ucvtf d1, d0", 0x7e61_d801);
    check!("ucvtf s1, s0", 0x7e21_d801);
    check!("scvtf v1.2s, v0.2s", 0x0e21_d801);
    check!("scvtf v1.4s, v0.4s", 0x4e21_d801);
    check!("scvtf v1.2d, v0.2d", 0x4e61_d801);
    check!("ucvtf v1.2s, v0.2s", 0x2e21_d801);
    check!("ucvtf v1.4s, v0.4s", 0x6e21_d801);
    check!("ucvtf v1.2d, v0.2d", 0x6e61_d801);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_reversals_match_native_all_widths() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for left in [0u128, u128::MAX, 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210] {
                for right in [1u128, u128::MAX, 0x0101_0101_0101_0101_0101_0101_0101_0101] {
                    let mut native = 0u128;
                    // SAFETY: valid 16-byte inputs/output and fixed instructions.
                    unsafe { std::arch::asm!(
                        "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("rev64 v2.8b, v0.8b", 0x0e200802);
    check!("rev64 v2.16b, v0.16b", 0x4e200802);
    check!("rev64 v2.4h, v0.4h", 0x0e600802);
    check!("rev64 v2.8h, v0.8h", 0x4e600802);
    check!("rev64 v2.2s, v0.2s", 0x0ea00802);
    check!("rev64 v2.4s, v0.4s", 0x4ea00802);
    check!("rev32 v2.8b, v0.8b", 0x2e200802);
    check!("rev32 v2.16b, v0.16b", 0x6e200802);
    check!("rev32 v2.4h, v0.4h", 0x2e600802);
    check!("rev32 v2.8h, v0.8h", 0x6e600802);
    check!("rev16 v2.8b, v0.8b", 0x0e201802);
    check!("rev16 v2.16b, v0.16b", 0x4e201802);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn floating_sign_and_move_operations_match_native() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {{
            let mut cpu = cpu_for($word);
            for source in [0u128, u128::MAX, 0x8000_0001_7fff_ffff_8000_0000_0100_0001,
                           0x1234_5678_9abc_def0_fedc_ba98_7654_3210, 0x7ff0_0000_0000_0001_7f80_0001_ff80_1234] {
                for control in 0..16u64 {
                    let fpcr = control << 22;
                    let mut native = 0u128;
                    let status: u64;
                    // SAFETY: fixed conversion, valid vector buffers, host FP restored.
                    unsafe { std::arch::asm!(
                        "mrs x8, fpcr", "mrs x9, fpsr", "msr fpcr, {control}", "msr fpsr, {initial}",
                        "ldr q0, [{source}]", $instruction, "str q1, [{output}]",
                        "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                        control = in(reg) fpcr, initial = in(reg) 2u64,
                        source = in(reg) &source, output = in(reg) &mut native,
                        status = out(reg) status, out("x8") _, out("x9") _, out("v0") _, out("v1") _,
                        options(nostack, preserves_flags),
                    ); }
                    cpu.pc = 0; cpu.v[0] = source; cpu.v[1] = u128::MAX;
                    cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 2;
                    cpu.step().unwrap();
                    assert_eq!((cpu.v[1], cpu.sysregs.fpsr), (native, status),
                        "{} source={source:#x} fpcr={fpcr:#x}", $instruction);
                }
            }
        }};
    }
    check!("fmov s1, s0", 0x1e204001);
    check!("fmov d1, d0", 0x1e604001);
    check!("fabs s1, s0", 0x1e20c001);
    check!("fabs d1, d0", 0x1e60c001);
    check!("fneg s1, s0", 0x1e214001);
    check!("fneg d1, d0", 0x1e614001);
    check!("fabs v1.2s, v0.2s", 0x0ea0f801);
    check!("fabs v1.4s, v0.4s", 0x4ea0f801);
    check!("fabs v1.2d, v0.2d", 0x4ee0f801);
    check!("fneg v1.2s, v0.2s", 0x2ea0f801);
    check!("fneg v1.4s, v0.4s", 0x6ea0f801);
    check!("fneg v1.2d, v0.2d", 0x6ee0f801);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn floating_conditional_select_matches_native_all_flags() {
    #[inline(never)]
    fn check<const WORD: u32>() {
        let mut cpu = cpu_for(WORD);
        for (left, right) in [
            (0u64, u64::MAX),
            (0x7ff0_0000_7f80_0001, 0x8000_0000_8000_0000),
            (0x1234_5678_9abc_def0, 0xfedc_ba98_7654_3210),
        ] {
            for nzcv in 0..16u8 {
                let mut native = u128::MAX;
                let status: u64;
                // SAFETY: fixed FCSEL instruction, valid output, host state restored.
                unsafe {
                    std::arch::asm!(
                        "mrs x8, fpcr", "mrs x9, fpsr", "mrs x11, nzcv",
                        "msr fpcr, {control}", "msr fpsr, {initial}", "msr nzcv, {nzcv}",
                        "fmov d0, {left}", "fmov d1, {right}", ".inst {word}", "str q2, [{output}]",
                        "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9", "msr nzcv, x11",
                        word = const WORD, control = in(reg) 0x0300_0000u64, initial = in(reg) 2u64,
                        nzcv = in(reg) (u64::from(nzcv) << 28), left = in(reg) left, right = in(reg) right,
                        output = in(reg) &mut native, status = out(reg) status,
                        out("x8") _, out("x9") _, out("x11") _, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    );
                }
                cpu.pc = 0;
                cpu.v[0] = u128::from(left);
                cpu.v[1] = u128::from(right);
                cpu.v[2] = u128::MAX;
                cpu.nzcv = nzcv;
                cpu.sysregs.fpcr = 0x0300_0000;
                cpu.sysregs.fpsr = 2;
                cpu.step().unwrap();
                assert_eq!(
                    (cpu.v[2], cpu.sysregs.fpsr),
                    (native, status),
                    "word={WORD:#x} nzcv={nzcv}"
                );
                assert_eq!(cpu.nzcv, nzcv);
            }
        }
    }
    check::<0x1e210c02>();
    check::<0x1e211c02>();
    check::<0x1e212c02>();
    check::<0x1e213c02>();
    check::<0x1e214c02>();
    check::<0x1e215c02>();
    check::<0x1e216c02>();
    check::<0x1e217c02>();
    check::<0x1e218c02>();
    check::<0x1e219c02>();
    check::<0x1e21ac02>();
    check::<0x1e21bc02>();
    check::<0x1e21cc02>();
    check::<0x1e21dc02>();
    check::<0x1e21ec02>();
    check::<0x1e21fc02>();
    check::<0x1e610c02>();
    check::<0x1e611c02>();
    check::<0x1e612c02>();
    check::<0x1e613c02>();
    check::<0x1e614c02>();
    check::<0x1e615c02>();
    check::<0x1e616c02>();
    check::<0x1e617c02>();
    check::<0x1e618c02>();
    check::<0x1e619c02>();
    check::<0x1e61ac02>();
    check::<0x1e61bc02>();
    check::<0x1e61cc02>();
    check::<0x1e61dc02>();
    check::<0x1e61ec02>();
    check::<0x1e61fc02>();
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn vector_floating_arithmetic_matches_native_lane_results_and_flags() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {{
            let mut cpu = cpu_for($word);
            let samples = [0u128, u128::MAX, 0x7ff0_0000_0000_0001_7f80_0000_0000_0000,
                0x0000_0000_0000_0001_0080_0000_007f_ffff,
                0x8000_0000_0000_0000_8000_0000_8000_0000,
                0x3ff0_0000_0000_0000_3f80_0000_3f00_0000];
            for left in samples { for right in samples { for control in 0..16u64 {
                let fpcr = control << 22;
                let mut native = 0u128;
                let status: u64;
                // SAFETY: fixed SIMD arithmetic; valid buffers, host FP restored.
                unsafe { std::arch::asm!(
                    "mrs x8, fpcr", "mrs x9, fpsr", "msr fpcr, {control}", "msr fpsr, {initial}",
                    "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction, "str q2, [{output}]",
                    "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                    control = in(reg) fpcr, initial = in(reg) 0x0800_0000u64,
                    left = in(reg) &left, right = in(reg) &right, output = in(reg) &mut native,
                    status = out(reg) status, out("x8") _, out("x9") _, out("v0") _, out("v1") _, out("v2") _,
                    options(nostack, preserves_flags),
                ); }
                cpu.pc = 0; cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 0x0800_0000;
                cpu.step().unwrap();
                assert_eq!((cpu.v[2], cpu.sysregs.fpsr), (native, status),
                    "{} left={left:#x} right={right:#x} fpcr={fpcr:#x}", $instruction);
            }}}
        }};
    }
    check!("fadd v2.2s, v0.2s, v1.2s", 0x0e21_d402);
    check!("fadd v2.4s, v0.4s, v1.4s", 0x4e21_d402);
    check!("fadd v2.2d, v0.2d, v1.2d", 0x4e61_d402);
    check!("fsub v2.2s, v0.2s, v1.2s", 0x0ea1_d402);
    check!("fsub v2.4s, v0.4s, v1.4s", 0x4ea1_d402);
    check!("fsub v2.2d, v0.2d, v1.2d", 0x4ee1_d402);
    check!("fmul v2.2s, v0.2s, v1.2s", 0x2e21_dc02);
    check!("fmul v2.4s, v0.4s, v1.4s", 0x6e21_dc02);
    check!("fmul v2.2d, v0.2d, v1.2d", 0x6e61_dc02);
    check!("fdiv v2.2s, v0.2s, v1.2s", 0x2e21_fc02);
    check!("fdiv v2.4s, v0.4s, v1.4s", 0x6e21_fc02);
    check!("fdiv v2.2d, v0.2d, v1.2d", 0x6e61_fc02);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn simd_float_to_integer_matches_native_all_rounding_forms() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {{
            let mut cpu = cpu_for($word);
            for source in [0u128, u128::MAX, 0x8000_0001_7fff_ffff_8000_0000_0100_0001,
                           0x43f0_0000_0000_0000_4f80_0000_cf00_0000] {
                for control in 0..16u64 {
                    let fpcr = control << 22;
                    let mut native = 0u128;
                    let status: u64;
                    // SAFETY: fixed conversion, valid vector buffers, host FP restored.
                    unsafe { std::arch::asm!(
                        "mrs x8, fpcr", "mrs x9, fpsr", "msr fpcr, {control}", "msr fpsr, {initial}",
                        "ldr q0, [{source}]", $instruction, "str q1, [{output}]",
                        "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                        control = in(reg) fpcr, initial = in(reg) 2u64,
                        source = in(reg) &source, output = in(reg) &mut native,
                        status = out(reg) status, out("x8") _, out("x9") _, out("v0") _, out("v1") _,
                        options(nostack, preserves_flags),
                    ); }
                    cpu.pc = 0; cpu.v[0] = source; cpu.v[1] = u128::MAX;
                    cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 2;
                    cpu.step().unwrap();
                    assert_eq!((cpu.v[1], cpu.sysregs.fpsr), (native, status),
                        "{} source={source:#x} fpcr={fpcr:#x}", $instruction);
                }
            }
        }};
    }
    check!("fcvtns s1, s0", 0x5e21a801);
    check!("fcvtns d1, d0", 0x5e61a801);
    check!("fcvtns v1.2s, v0.2s", 0x0e21a801);
    check!("fcvtns v1.4s, v0.4s", 0x4e21a801);
    check!("fcvtns v1.2d, v0.2d", 0x4e61a801);
    check!("fcvtnu s1, s0", 0x7e21a801);
    check!("fcvtnu d1, d0", 0x7e61a801);
    check!("fcvtnu v1.2s, v0.2s", 0x2e21a801);
    check!("fcvtnu v1.4s, v0.4s", 0x6e21a801);
    check!("fcvtnu v1.2d, v0.2d", 0x6e61a801);
    check!("fcvtas s1, s0", 0x5e21c801);
    check!("fcvtas d1, d0", 0x5e61c801);
    check!("fcvtas v1.2s, v0.2s", 0x0e21c801);
    check!("fcvtas v1.4s, v0.4s", 0x4e21c801);
    check!("fcvtas v1.2d, v0.2d", 0x4e61c801);
    check!("fcvtau s1, s0", 0x7e21c801);
    check!("fcvtau d1, d0", 0x7e61c801);
    check!("fcvtau v1.2s, v0.2s", 0x2e21c801);
    check!("fcvtau v1.4s, v0.4s", 0x6e21c801);
    check!("fcvtau v1.2d, v0.2d", 0x6e61c801);
    check!("fcvtps s1, s0", 0x5ea1a801);
    check!("fcvtps d1, d0", 0x5ee1a801);
    check!("fcvtps v1.2s, v0.2s", 0x0ea1a801);
    check!("fcvtps v1.4s, v0.4s", 0x4ea1a801);
    check!("fcvtps v1.2d, v0.2d", 0x4ee1a801);
    check!("fcvtpu s1, s0", 0x7ea1a801);
    check!("fcvtpu d1, d0", 0x7ee1a801);
    check!("fcvtpu v1.2s, v0.2s", 0x2ea1a801);
    check!("fcvtpu v1.4s, v0.4s", 0x6ea1a801);
    check!("fcvtpu v1.2d, v0.2d", 0x6ee1a801);
    check!("fcvtms s1, s0", 0x5e21b801);
    check!("fcvtms d1, d0", 0x5e61b801);
    check!("fcvtms v1.2s, v0.2s", 0x0e21b801);
    check!("fcvtms v1.4s, v0.4s", 0x4e21b801);
    check!("fcvtms v1.2d, v0.2d", 0x4e61b801);
    check!("fcvtmu s1, s0", 0x7e21b801);
    check!("fcvtmu d1, d0", 0x7e61b801);
    check!("fcvtmu v1.2s, v0.2s", 0x2e21b801);
    check!("fcvtmu v1.4s, v0.4s", 0x6e21b801);
    check!("fcvtmu v1.2d, v0.2d", 0x6e61b801);
    check!("fcvtzs s1, s0", 0x5ea1b801);
    check!("fcvtzs d1, d0", 0x5ee1b801);
    check!("fcvtzs v1.2s, v0.2s", 0x0ea1b801);
    check!("fcvtzs v1.4s, v0.4s", 0x4ea1b801);
    check!("fcvtzs v1.2d, v0.2d", 0x4ee1b801);
    check!("fcvtzu s1, s0", 0x7ea1b801);
    check!("fcvtzu d1, d0", 0x7ee1b801);
    check!("fcvtzu v1.2s, v0.2s", 0x2ea1b801);
    check!("fcvtzu v1.4s, v0.4s", 0x6ea1b801);
    check!("fcvtzu v1.2d, v0.2d", 0x6ee1b801);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn scalar_round_to_integral_matches_native_all_controls() {
    macro_rules! check {
        ($op:expr, $word:expr, $double:expr, $load:literal, $store:literal) => {{
            #[inline(never)]
            fn run() {
                let mut cpu = cpu_for($word);
                let special: &[u64] = if $double {
                    &[0, 1, 0x000f_ffff_ffff_ffff, 0x0010_0000_0000_0000,
                      0x3fdf_ffff_ffff_ffff, 0x3fe0_0000_0000_0000, 0x3fe0_0000_0000_0001,
                      0x3ff8_0000_0000_0000, 0x4004_0000_0000_0000, 0x433f_ffff_ffff_ffff,
                      0x7fef_ffff_ffff_ffff, 0x7ff0_0000_0000_0000,
                      0x7ff0_0000_0000_1234, 0x7ff8_0000_0000_5678]
                } else {
                    &[0, 1, 0x007f_ffff, 0x0080_0000, 0x3eff_ffff, 0x3f00_0000,
                      0x3f00_0001, 0x3fc0_0000, 0x4020_0000, 0x4b7f_ffff,
                      0x7f7f_ffff, 0x7f80_0000, 0x7f80_1234, 0x7fc0_5678]
                };
                let sign = if $double { 1u64 << 63 } else { 1u64 << 31 };
                let mut inputs = Vec::new();
                for &value in special { inputs.extend([value, value | sign]); }
                let mut random = 0x4798_bade_0147_ac13u64;
                for _ in 0..1024 {
                    random ^= random << 13; random ^= random >> 7; random ^= random << 17;
                    inputs.push(random);
                }
                for control in 0..16u64 {
                    let fpcr = control << 22;
                    for &input in &inputs {
                        let native: u64;
                        let status: u64;
                        // SAFETY: fixed instructions only; host FP state is restored.
                        unsafe { std::arch::asm!(
                            "mrs x8, fpcr", "mrs x9, fpsr", "msr fpcr, {control}", "msr fpsr, {initial}",
                            $load, $op, $store,
                            "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9", "mov {result}, x10",
                            control = in(reg) fpcr, initial = in(reg) 0x0800_0000u64,
                            input = in(reg) input, result = out(reg) native, status = out(reg) status,
                            out("x8") _, out("x9") _, out("x10") _, out("v0") _, out("v1") _,
                            options(nostack, nomem, preserves_flags),
                        ); }
                        cpu.pc = 0; cpu.v[0] = u128::from(input); cpu.v[1] = u128::MAX;
                        cpu.sysregs.fpcr = fpcr; cpu.sysregs.fpsr = 0x0800_0000;
                        cpu.step().unwrap();
                        assert_eq!((cpu.v[1], cpu.sysregs.fpsr), (u128::from(native), status),
                            "{} input={input:#x} fpcr={fpcr:#x}", $op);
                    }
                }
            }
            run();
        }};
    }
    macro_rules! both {
        ($op:literal, $word:expr) => {
            check!(
                concat!($op, " s1, s0"),
                $word,
                false,
                "fmov s0, {input:w}",
                "fmov w10, s1"
            );
            check!(
                concat!($op, " d1, d0"),
                $word | (1 << 22),
                true,
                "fmov d0, {input}",
                "fmov x10, d1"
            );
        };
    }
    both!("frintn", 0x1e24_4001);
    both!("frintp", 0x1e24_c001);
    both!("frintm", 0x1e25_4001);
    both!("frintz", 0x1e25_c001);
    both!("frinta", 0x1e26_4001);
    both!("frintx", 0x1e27_4001);
    both!("frinti", 0x1e27_c001);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn widening_add_subtract_matches_native_all_forms() {
    #[inline(never)]
    fn check<const WORD: u32>(reference: fn(u128, u128) -> u128) {
        let mut cpu = cpu_for(WORD);
        let patterns = [
            0u128,
            u128::MAX,
            0x7fff_8000_ffff_0001_80ff_0100_7f80_ff01,
            0x8000_0000_7fff_ffff_0000_0001_ffff_ffff,
        ];
        for left in patterns {
            for right in patterns {
                let native = reference(left, right);
                cpu.pc = 0;
                cpu.v[0] = left;
                cpu.v[1] = right;
                cpu.v[2] = 123;
                cpu.sysregs.fpsr = 0x0800_009f;
                cpu.nzcv = 0xa;
                cpu.step().unwrap();
                assert_eq!(
                    cpu.v[2], native,
                    "word={WORD:#x} left={left:#x} right={right:#x}"
                );
                assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                assert_eq!(cpu.nzcv, 0xa);
            }
        }
    }
    macro_rules! case {
        ($instruction:literal, $word:expr) => {
            check::<$word>(|left, right| {
                let mut result = 0u128;
                // SAFETY: fixed assembly with valid local 16-byte buffers.
                unsafe { std::arch::asm!(
                    "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                    "str q2, [{result}]",
                    left = in(reg) &left, right = in(reg) &right, result = in(reg) &mut result,
                    out("v0") _, out("v1") _, out("v2") _, options(nostack, preserves_flags),
                ); }
                result
            });
        };
    }
    case!("saddl v2.8h, v0.8b, v1.8b", 0x0e210002);
    case!("saddl v2.4s, v0.4h, v1.4h", 0x0e610002);
    case!("saddl v2.2d, v0.2s, v1.2s", 0x0ea10002);
    case!("saddl2 v2.8h, v0.16b, v1.16b", 0x4e210002);
    case!("saddl2 v2.4s, v0.8h, v1.8h", 0x4e610002);
    case!("saddl2 v2.2d, v0.4s, v1.4s", 0x4ea10002);
    case!("uaddl v2.8h, v0.8b, v1.8b", 0x2e210002);
    case!("uaddl v2.4s, v0.4h, v1.4h", 0x2e610002);
    case!("uaddl v2.2d, v0.2s, v1.2s", 0x2ea10002);
    case!("uaddl2 v2.8h, v0.16b, v1.16b", 0x6e210002);
    case!("uaddl2 v2.4s, v0.8h, v1.8h", 0x6e610002);
    case!("uaddl2 v2.2d, v0.4s, v1.4s", 0x6ea10002);
    case!("ssubl v2.8h, v0.8b, v1.8b", 0x0e212002);
    case!("ssubl v2.4s, v0.4h, v1.4h", 0x0e612002);
    case!("ssubl v2.2d, v0.2s, v1.2s", 0x0ea12002);
    case!("ssubl2 v2.8h, v0.16b, v1.16b", 0x4e212002);
    case!("ssubl2 v2.4s, v0.8h, v1.8h", 0x4e612002);
    case!("ssubl2 v2.2d, v0.4s, v1.4s", 0x4ea12002);
    case!("usubl v2.8h, v0.8b, v1.8b", 0x2e212002);
    case!("usubl v2.4s, v0.4h, v1.4h", 0x2e612002);
    case!("usubl v2.2d, v0.2s, v1.2s", 0x2ea12002);
    case!("usubl2 v2.8h, v0.16b, v1.16b", 0x6e212002);
    case!("usubl2 v2.4s, v0.8h, v1.8h", 0x6e612002);
    case!("usubl2 v2.2d, v0.4s, v1.4s", 0x6ea12002);
    case!("saddw v2.8h, v0.8h, v1.8b", 0x0e211002);
    case!("saddw v2.4s, v0.4s, v1.4h", 0x0e611002);
    case!("saddw v2.2d, v0.2d, v1.2s", 0x0ea11002);
    case!("saddw2 v2.8h, v0.8h, v1.16b", 0x4e211002);
    case!("saddw2 v2.4s, v0.4s, v1.8h", 0x4e611002);
    case!("saddw2 v2.2d, v0.2d, v1.4s", 0x4ea11002);
    case!("uaddw v2.8h, v0.8h, v1.8b", 0x2e211002);
    case!("uaddw v2.4s, v0.4s, v1.4h", 0x2e611002);
    case!("uaddw v2.2d, v0.2d, v1.2s", 0x2ea11002);
    case!("uaddw2 v2.8h, v0.8h, v1.16b", 0x6e211002);
    case!("uaddw2 v2.4s, v0.4s, v1.8h", 0x6e611002);
    case!("uaddw2 v2.2d, v0.2d, v1.4s", 0x6ea11002);
    case!("ssubw v2.8h, v0.8h, v1.8b", 0x0e213002);
    case!("ssubw v2.4s, v0.4s, v1.4h", 0x0e613002);
    case!("ssubw v2.2d, v0.2d, v1.2s", 0x0ea13002);
    case!("ssubw2 v2.8h, v0.8h, v1.16b", 0x4e213002);
    case!("ssubw2 v2.4s, v0.4s, v1.8h", 0x4e613002);
    case!("ssubw2 v2.2d, v0.2d, v1.4s", 0x4ea13002);
    case!("usubw v2.8h, v0.8h, v1.8b", 0x2e213002);
    case!("usubw v2.4s, v0.4s, v1.4h", 0x2e613002);
    case!("usubw v2.2d, v0.2d, v1.2s", 0x2ea13002);
    case!("usubw2 v2.8h, v0.8h, v1.16b", 0x6e213002);
    case!("usubw2 v2.4s, v0.4s, v1.8h", 0x6e613002);
    case!("usubw2 v2.2d, v0.2d, v1.4s", 0x6ea13002);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn structure_load_store_matches_native_all_forms() {
    #[inline(never)]
    fn check<const WORD: u32>(reference: fn(&mut [u8; 128], &mut [u128; 4]) -> usize) {
        let original: [u8; 128] = std::array::from_fn(|i| (i as u8).wrapping_mul(37));
        let vectors = [0x123456789abcdef0u128, u128::MAX, 1u128 << 127, 0x55aa];
        let mut native_memory = original;
        let mut native_vectors = vectors;
        let advance = reference(&mut native_memory, &mut native_vectors);
        let mut cpu = cpu_for(WORD);
        cpu.memory.write(0x100, &original).unwrap();
        cpu.v[..4].copy_from_slice(&vectors);
        cpu.set_x(1, 0x100);
        cpu.set_x(4, 37);
        cpu.nzcv = 0xa;
        cpu.sysregs.fpsr = 0x0800_009f;
        cpu.step().unwrap();
        let mut actual = [0; 128];
        cpu.memory.read(0x100, &mut actual).unwrap();
        assert_eq!(actual, native_memory, "memory word={WORD:#x}");
        assert_eq!(&cpu.v[..4], &native_vectors, "vectors word={WORD:#x}");
        assert_eq!(cpu.x(1), 0x100 + advance as u64, "writeback word={WORD:#x}");
        assert_eq!(cpu.nzcv, 0xa);
        assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
        assert_eq!(cpu.pc, 4);
    }
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            check::<$word>(|memory, vectors| {
                let start = memory.as_mut_ptr() as usize;
                let mut base = start;
                // SAFETY: fixed asm accesses at most 64 bytes in the 128-byte
                // initialized buffer; writeback remains inside that allocation.
                unsafe {
                    std::arch::asm!(
                        "ldp q0, q1, [{vectors}]", "ldp q2, q3, [{vectors}, #32]",
                        $instruction,
                        "stp q0, q1, [{vectors}]", "stp q2, q3, [{vectors}, #32]",
                        vectors = in(reg) vectors.as_mut_ptr(),
                        inout("x1") base, in("x4") 37usize,
                        out("v0") _, out("v1") _, out("v2") _, out("v3") _,
                        options(nostack, preserves_flags),
                    );
                }
                base - start
            });
        };
    }
    check!("ld1 {{v0.8b}}, [x1]", 0x0c407020);
    check!("ld1 {{v0.8b}}, [x1], #8", 0x0cdf7020);
    check!("ld1 {{v0.8b}}, [x1], x4", 0x0cc47020);
    check!("ld1 {{v0.16b}}, [x1]", 0x4c407020);
    check!("ld1 {{v0.16b}}, [x1], #16", 0x4cdf7020);
    check!("ld1 {{v0.16b}}, [x1], x4", 0x4cc47020);
    check!("ld1 {{v0.4h}}, [x1]", 0x0c407420);
    check!("ld1 {{v0.4h}}, [x1], #8", 0x0cdf7420);
    check!("ld1 {{v0.4h}}, [x1], x4", 0x0cc47420);
    check!("ld1 {{v0.8h}}, [x1]", 0x4c407420);
    check!("ld1 {{v0.8h}}, [x1], #16", 0x4cdf7420);
    check!("ld1 {{v0.8h}}, [x1], x4", 0x4cc47420);
    check!("ld1 {{v0.2s}}, [x1]", 0x0c407820);
    check!("ld1 {{v0.2s}}, [x1], #8", 0x0cdf7820);
    check!("ld1 {{v0.2s}}, [x1], x4", 0x0cc47820);
    check!("ld1 {{v0.4s}}, [x1]", 0x4c407820);
    check!("ld1 {{v0.4s}}, [x1], #16", 0x4cdf7820);
    check!("ld1 {{v0.4s}}, [x1], x4", 0x4cc47820);
    check!("ld1 {{v0.1d}}, [x1]", 0x0c407c20);
    check!("ld1 {{v0.1d}}, [x1], #8", 0x0cdf7c20);
    check!("ld1 {{v0.1d}}, [x1], x4", 0x0cc47c20);
    check!("ld1 {{v0.2d}}, [x1]", 0x4c407c20);
    check!("ld1 {{v0.2d}}, [x1], #16", 0x4cdf7c20);
    check!("ld1 {{v0.2d}}, [x1], x4", 0x4cc47c20);
    check!("ld1 {{v0.8b, v1.8b}}, [x1]", 0x0c40a020);
    check!("ld1 {{v0.8b, v1.8b}}, [x1], #16", 0x0cdfa020);
    check!("ld1 {{v0.8b, v1.8b}}, [x1], x4", 0x0cc4a020);
    check!("ld1 {{v0.16b, v1.16b}}, [x1]", 0x4c40a020);
    check!("ld1 {{v0.16b, v1.16b}}, [x1], #32", 0x4cdfa020);
    check!("ld1 {{v0.16b, v1.16b}}, [x1], x4", 0x4cc4a020);
    check!("ld1 {{v0.4h, v1.4h}}, [x1]", 0x0c40a420);
    check!("ld1 {{v0.4h, v1.4h}}, [x1], #16", 0x0cdfa420);
    check!("ld1 {{v0.4h, v1.4h}}, [x1], x4", 0x0cc4a420);
    check!("ld1 {{v0.8h, v1.8h}}, [x1]", 0x4c40a420);
    check!("ld1 {{v0.8h, v1.8h}}, [x1], #32", 0x4cdfa420);
    check!("ld1 {{v0.8h, v1.8h}}, [x1], x4", 0x4cc4a420);
    check!("ld1 {{v0.2s, v1.2s}}, [x1]", 0x0c40a820);
    check!("ld1 {{v0.2s, v1.2s}}, [x1], #16", 0x0cdfa820);
    check!("ld1 {{v0.2s, v1.2s}}, [x1], x4", 0x0cc4a820);
    check!("ld1 {{v0.4s, v1.4s}}, [x1]", 0x4c40a820);
    check!("ld1 {{v0.4s, v1.4s}}, [x1], #32", 0x4cdfa820);
    check!("ld1 {{v0.4s, v1.4s}}, [x1], x4", 0x4cc4a820);
    check!("ld1 {{v0.1d, v1.1d}}, [x1]", 0x0c40ac20);
    check!("ld1 {{v0.1d, v1.1d}}, [x1], #16", 0x0cdfac20);
    check!("ld1 {{v0.1d, v1.1d}}, [x1], x4", 0x0cc4ac20);
    check!("ld1 {{v0.2d, v1.2d}}, [x1]", 0x4c40ac20);
    check!("ld1 {{v0.2d, v1.2d}}, [x1], #32", 0x4cdfac20);
    check!("ld1 {{v0.2d, v1.2d}}, [x1], x4", 0x4cc4ac20);
    check!("ld1 {{v0.8b, v1.8b, v2.8b}}, [x1]", 0x0c406020);
    check!("ld1 {{v0.8b, v1.8b, v2.8b}}, [x1], #24", 0x0cdf6020);
    check!("ld1 {{v0.8b, v1.8b, v2.8b}}, [x1], x4", 0x0cc46020);
    check!("ld1 {{v0.16b, v1.16b, v2.16b}}, [x1]", 0x4c406020);
    check!("ld1 {{v0.16b, v1.16b, v2.16b}}, [x1], #48", 0x4cdf6020);
    check!("ld1 {{v0.16b, v1.16b, v2.16b}}, [x1], x4", 0x4cc46020);
    check!("ld1 {{v0.4h, v1.4h, v2.4h}}, [x1]", 0x0c406420);
    check!("ld1 {{v0.4h, v1.4h, v2.4h}}, [x1], #24", 0x0cdf6420);
    check!("ld1 {{v0.4h, v1.4h, v2.4h}}, [x1], x4", 0x0cc46420);
    check!("ld1 {{v0.8h, v1.8h, v2.8h}}, [x1]", 0x4c406420);
    check!("ld1 {{v0.8h, v1.8h, v2.8h}}, [x1], #48", 0x4cdf6420);
    check!("ld1 {{v0.8h, v1.8h, v2.8h}}, [x1], x4", 0x4cc46420);
    check!("ld1 {{v0.2s, v1.2s, v2.2s}}, [x1]", 0x0c406820);
    check!("ld1 {{v0.2s, v1.2s, v2.2s}}, [x1], #24", 0x0cdf6820);
    check!("ld1 {{v0.2s, v1.2s, v2.2s}}, [x1], x4", 0x0cc46820);
    check!("ld1 {{v0.4s, v1.4s, v2.4s}}, [x1]", 0x4c406820);
    check!("ld1 {{v0.4s, v1.4s, v2.4s}}, [x1], #48", 0x4cdf6820);
    check!("ld1 {{v0.4s, v1.4s, v2.4s}}, [x1], x4", 0x4cc46820);
    check!("ld1 {{v0.1d, v1.1d, v2.1d}}, [x1]", 0x0c406c20);
    check!("ld1 {{v0.1d, v1.1d, v2.1d}}, [x1], #24", 0x0cdf6c20);
    check!("ld1 {{v0.1d, v1.1d, v2.1d}}, [x1], x4", 0x0cc46c20);
    check!("ld1 {{v0.2d, v1.2d, v2.2d}}, [x1]", 0x4c406c20);
    check!("ld1 {{v0.2d, v1.2d, v2.2d}}, [x1], #48", 0x4cdf6c20);
    check!("ld1 {{v0.2d, v1.2d, v2.2d}}, [x1], x4", 0x4cc46c20);
    check!("ld1 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1]", 0x0c402020);
    check!("ld1 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], #32", 0x0cdf2020);
    check!("ld1 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], x4", 0x0cc42020);
    check!("ld1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1]", 0x4c402020);
    check!(
        "ld1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], #64",
        0x4cdf2020
    );
    check!(
        "ld1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], x4",
        0x4cc42020
    );
    check!("ld1 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1]", 0x0c402420);
    check!("ld1 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], #32", 0x0cdf2420);
    check!("ld1 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], x4", 0x0cc42420);
    check!("ld1 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1]", 0x4c402420);
    check!("ld1 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], #64", 0x4cdf2420);
    check!("ld1 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], x4", 0x4cc42420);
    check!("ld1 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1]", 0x0c402820);
    check!("ld1 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], #32", 0x0cdf2820);
    check!("ld1 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], x4", 0x0cc42820);
    check!("ld1 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1]", 0x4c402820);
    check!("ld1 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], #64", 0x4cdf2820);
    check!("ld1 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], x4", 0x4cc42820);
    check!("ld1 {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1]", 0x0c402c20);
    check!("ld1 {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1], #32", 0x0cdf2c20);
    check!("ld1 {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1], x4", 0x0cc42c20);
    check!("ld1 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1]", 0x4c402c20);
    check!("ld1 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], #64", 0x4cdf2c20);
    check!("ld1 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], x4", 0x4cc42c20);
    check!("st1 {{v0.8b}}, [x1]", 0x0c007020);
    check!("st1 {{v0.8b}}, [x1], #8", 0x0c9f7020);
    check!("st1 {{v0.8b}}, [x1], x4", 0x0c847020);
    check!("st1 {{v0.16b}}, [x1]", 0x4c007020);
    check!("st1 {{v0.16b}}, [x1], #16", 0x4c9f7020);
    check!("st1 {{v0.16b}}, [x1], x4", 0x4c847020);
    check!("st1 {{v0.4h}}, [x1]", 0x0c007420);
    check!("st1 {{v0.4h}}, [x1], #8", 0x0c9f7420);
    check!("st1 {{v0.4h}}, [x1], x4", 0x0c847420);
    check!("st1 {{v0.8h}}, [x1]", 0x4c007420);
    check!("st1 {{v0.8h}}, [x1], #16", 0x4c9f7420);
    check!("st1 {{v0.8h}}, [x1], x4", 0x4c847420);
    check!("st1 {{v0.2s}}, [x1]", 0x0c007820);
    check!("st1 {{v0.2s}}, [x1], #8", 0x0c9f7820);
    check!("st1 {{v0.2s}}, [x1], x4", 0x0c847820);
    check!("st1 {{v0.4s}}, [x1]", 0x4c007820);
    check!("st1 {{v0.4s}}, [x1], #16", 0x4c9f7820);
    check!("st1 {{v0.4s}}, [x1], x4", 0x4c847820);
    check!("st1 {{v0.1d}}, [x1]", 0x0c007c20);
    check!("st1 {{v0.1d}}, [x1], #8", 0x0c9f7c20);
    check!("st1 {{v0.1d}}, [x1], x4", 0x0c847c20);
    check!("st1 {{v0.2d}}, [x1]", 0x4c007c20);
    check!("st1 {{v0.2d}}, [x1], #16", 0x4c9f7c20);
    check!("st1 {{v0.2d}}, [x1], x4", 0x4c847c20);
    check!("st1 {{v0.8b, v1.8b}}, [x1]", 0x0c00a020);
    check!("st1 {{v0.8b, v1.8b}}, [x1], #16", 0x0c9fa020);
    check!("st1 {{v0.8b, v1.8b}}, [x1], x4", 0x0c84a020);
    check!("st1 {{v0.16b, v1.16b}}, [x1]", 0x4c00a020);
    check!("st1 {{v0.16b, v1.16b}}, [x1], #32", 0x4c9fa020);
    check!("st1 {{v0.16b, v1.16b}}, [x1], x4", 0x4c84a020);
    check!("st1 {{v0.4h, v1.4h}}, [x1]", 0x0c00a420);
    check!("st1 {{v0.4h, v1.4h}}, [x1], #16", 0x0c9fa420);
    check!("st1 {{v0.4h, v1.4h}}, [x1], x4", 0x0c84a420);
    check!("st1 {{v0.8h, v1.8h}}, [x1]", 0x4c00a420);
    check!("st1 {{v0.8h, v1.8h}}, [x1], #32", 0x4c9fa420);
    check!("st1 {{v0.8h, v1.8h}}, [x1], x4", 0x4c84a420);
    check!("st1 {{v0.2s, v1.2s}}, [x1]", 0x0c00a820);
    check!("st1 {{v0.2s, v1.2s}}, [x1], #16", 0x0c9fa820);
    check!("st1 {{v0.2s, v1.2s}}, [x1], x4", 0x0c84a820);
    check!("st1 {{v0.4s, v1.4s}}, [x1]", 0x4c00a820);
    check!("st1 {{v0.4s, v1.4s}}, [x1], #32", 0x4c9fa820);
    check!("st1 {{v0.4s, v1.4s}}, [x1], x4", 0x4c84a820);
    check!("st1 {{v0.1d, v1.1d}}, [x1]", 0x0c00ac20);
    check!("st1 {{v0.1d, v1.1d}}, [x1], #16", 0x0c9fac20);
    check!("st1 {{v0.1d, v1.1d}}, [x1], x4", 0x0c84ac20);
    check!("st1 {{v0.2d, v1.2d}}, [x1]", 0x4c00ac20);
    check!("st1 {{v0.2d, v1.2d}}, [x1], #32", 0x4c9fac20);
    check!("st1 {{v0.2d, v1.2d}}, [x1], x4", 0x4c84ac20);
    check!("st1 {{v0.8b, v1.8b, v2.8b}}, [x1]", 0x0c006020);
    check!("st1 {{v0.8b, v1.8b, v2.8b}}, [x1], #24", 0x0c9f6020);
    check!("st1 {{v0.8b, v1.8b, v2.8b}}, [x1], x4", 0x0c846020);
    check!("st1 {{v0.16b, v1.16b, v2.16b}}, [x1]", 0x4c006020);
    check!("st1 {{v0.16b, v1.16b, v2.16b}}, [x1], #48", 0x4c9f6020);
    check!("st1 {{v0.16b, v1.16b, v2.16b}}, [x1], x4", 0x4c846020);
    check!("st1 {{v0.4h, v1.4h, v2.4h}}, [x1]", 0x0c006420);
    check!("st1 {{v0.4h, v1.4h, v2.4h}}, [x1], #24", 0x0c9f6420);
    check!("st1 {{v0.4h, v1.4h, v2.4h}}, [x1], x4", 0x0c846420);
    check!("st1 {{v0.8h, v1.8h, v2.8h}}, [x1]", 0x4c006420);
    check!("st1 {{v0.8h, v1.8h, v2.8h}}, [x1], #48", 0x4c9f6420);
    check!("st1 {{v0.8h, v1.8h, v2.8h}}, [x1], x4", 0x4c846420);
    check!("st1 {{v0.2s, v1.2s, v2.2s}}, [x1]", 0x0c006820);
    check!("st1 {{v0.2s, v1.2s, v2.2s}}, [x1], #24", 0x0c9f6820);
    check!("st1 {{v0.2s, v1.2s, v2.2s}}, [x1], x4", 0x0c846820);
    check!("st1 {{v0.4s, v1.4s, v2.4s}}, [x1]", 0x4c006820);
    check!("st1 {{v0.4s, v1.4s, v2.4s}}, [x1], #48", 0x4c9f6820);
    check!("st1 {{v0.4s, v1.4s, v2.4s}}, [x1], x4", 0x4c846820);
    check!("st1 {{v0.1d, v1.1d, v2.1d}}, [x1]", 0x0c006c20);
    check!("st1 {{v0.1d, v1.1d, v2.1d}}, [x1], #24", 0x0c9f6c20);
    check!("st1 {{v0.1d, v1.1d, v2.1d}}, [x1], x4", 0x0c846c20);
    check!("st1 {{v0.2d, v1.2d, v2.2d}}, [x1]", 0x4c006c20);
    check!("st1 {{v0.2d, v1.2d, v2.2d}}, [x1], #48", 0x4c9f6c20);
    check!("st1 {{v0.2d, v1.2d, v2.2d}}, [x1], x4", 0x4c846c20);
    check!("st1 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1]", 0x0c002020);
    check!("st1 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], #32", 0x0c9f2020);
    check!("st1 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], x4", 0x0c842020);
    check!("st1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1]", 0x4c002020);
    check!(
        "st1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], #64",
        0x4c9f2020
    );
    check!(
        "st1 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], x4",
        0x4c842020
    );
    check!("st1 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1]", 0x0c002420);
    check!("st1 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], #32", 0x0c9f2420);
    check!("st1 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], x4", 0x0c842420);
    check!("st1 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1]", 0x4c002420);
    check!("st1 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], #64", 0x4c9f2420);
    check!("st1 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], x4", 0x4c842420);
    check!("st1 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1]", 0x0c002820);
    check!("st1 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], #32", 0x0c9f2820);
    check!("st1 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], x4", 0x0c842820);
    check!("st1 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1]", 0x4c002820);
    check!("st1 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], #64", 0x4c9f2820);
    check!("st1 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], x4", 0x4c842820);
    check!("st1 {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1]", 0x0c002c20);
    check!("st1 {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1], #32", 0x0c9f2c20);
    check!("st1 {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1], x4", 0x0c842c20);
    check!("st1 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1]", 0x4c002c20);
    check!("st1 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], #64", 0x4c9f2c20);
    check!("st1 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], x4", 0x4c842c20);
    check!("ld1 {{v0.b}}[0], [x1]", 0x0d400020);
    check!("ld1 {{v0.b}}[0], [x1], #1", 0x0ddf0020);
    check!("ld1 {{v0.b}}[0], [x1], x4", 0x0dc40020);
    check!("ld1 {{v0.b}}[1], [x1]", 0x0d400420);
    check!("ld1 {{v0.b}}[1], [x1], #1", 0x0ddf0420);
    check!("ld1 {{v0.b}}[1], [x1], x4", 0x0dc40420);
    check!("ld1 {{v0.b}}[2], [x1]", 0x0d400820);
    check!("ld1 {{v0.b}}[2], [x1], #1", 0x0ddf0820);
    check!("ld1 {{v0.b}}[2], [x1], x4", 0x0dc40820);
    check!("ld1 {{v0.b}}[3], [x1]", 0x0d400c20);
    check!("ld1 {{v0.b}}[3], [x1], #1", 0x0ddf0c20);
    check!("ld1 {{v0.b}}[3], [x1], x4", 0x0dc40c20);
    check!("ld1 {{v0.b}}[4], [x1]", 0x0d401020);
    check!("ld1 {{v0.b}}[4], [x1], #1", 0x0ddf1020);
    check!("ld1 {{v0.b}}[4], [x1], x4", 0x0dc41020);
    check!("ld1 {{v0.b}}[5], [x1]", 0x0d401420);
    check!("ld1 {{v0.b}}[5], [x1], #1", 0x0ddf1420);
    check!("ld1 {{v0.b}}[5], [x1], x4", 0x0dc41420);
    check!("ld1 {{v0.b}}[6], [x1]", 0x0d401820);
    check!("ld1 {{v0.b}}[6], [x1], #1", 0x0ddf1820);
    check!("ld1 {{v0.b}}[6], [x1], x4", 0x0dc41820);
    check!("ld1 {{v0.b}}[7], [x1]", 0x0d401c20);
    check!("ld1 {{v0.b}}[7], [x1], #1", 0x0ddf1c20);
    check!("ld1 {{v0.b}}[7], [x1], x4", 0x0dc41c20);
    check!("ld1 {{v0.b}}[8], [x1]", 0x4d400020);
    check!("ld1 {{v0.b}}[8], [x1], #1", 0x4ddf0020);
    check!("ld1 {{v0.b}}[8], [x1], x4", 0x4dc40020);
    check!("ld1 {{v0.b}}[9], [x1]", 0x4d400420);
    check!("ld1 {{v0.b}}[9], [x1], #1", 0x4ddf0420);
    check!("ld1 {{v0.b}}[9], [x1], x4", 0x4dc40420);
    check!("ld1 {{v0.b}}[10], [x1]", 0x4d400820);
    check!("ld1 {{v0.b}}[10], [x1], #1", 0x4ddf0820);
    check!("ld1 {{v0.b}}[10], [x1], x4", 0x4dc40820);
    check!("ld1 {{v0.b}}[11], [x1]", 0x4d400c20);
    check!("ld1 {{v0.b}}[11], [x1], #1", 0x4ddf0c20);
    check!("ld1 {{v0.b}}[11], [x1], x4", 0x4dc40c20);
    check!("ld1 {{v0.b}}[12], [x1]", 0x4d401020);
    check!("ld1 {{v0.b}}[12], [x1], #1", 0x4ddf1020);
    check!("ld1 {{v0.b}}[12], [x1], x4", 0x4dc41020);
    check!("ld1 {{v0.b}}[13], [x1]", 0x4d401420);
    check!("ld1 {{v0.b}}[13], [x1], #1", 0x4ddf1420);
    check!("ld1 {{v0.b}}[13], [x1], x4", 0x4dc41420);
    check!("ld1 {{v0.b}}[14], [x1]", 0x4d401820);
    check!("ld1 {{v0.b}}[14], [x1], #1", 0x4ddf1820);
    check!("ld1 {{v0.b}}[14], [x1], x4", 0x4dc41820);
    check!("ld1 {{v0.b}}[15], [x1]", 0x4d401c20);
    check!("ld1 {{v0.b}}[15], [x1], #1", 0x4ddf1c20);
    check!("ld1 {{v0.b}}[15], [x1], x4", 0x4dc41c20);
    check!("ld1 {{v0.h}}[0], [x1]", 0x0d404020);
    check!("ld1 {{v0.h}}[0], [x1], #2", 0x0ddf4020);
    check!("ld1 {{v0.h}}[0], [x1], x4", 0x0dc44020);
    check!("ld1 {{v0.h}}[1], [x1]", 0x0d404820);
    check!("ld1 {{v0.h}}[1], [x1], #2", 0x0ddf4820);
    check!("ld1 {{v0.h}}[1], [x1], x4", 0x0dc44820);
    check!("ld1 {{v0.h}}[2], [x1]", 0x0d405020);
    check!("ld1 {{v0.h}}[2], [x1], #2", 0x0ddf5020);
    check!("ld1 {{v0.h}}[2], [x1], x4", 0x0dc45020);
    check!("ld1 {{v0.h}}[3], [x1]", 0x0d405820);
    check!("ld1 {{v0.h}}[3], [x1], #2", 0x0ddf5820);
    check!("ld1 {{v0.h}}[3], [x1], x4", 0x0dc45820);
    check!("ld1 {{v0.h}}[4], [x1]", 0x4d404020);
    check!("ld1 {{v0.h}}[4], [x1], #2", 0x4ddf4020);
    check!("ld1 {{v0.h}}[4], [x1], x4", 0x4dc44020);
    check!("ld1 {{v0.h}}[5], [x1]", 0x4d404820);
    check!("ld1 {{v0.h}}[5], [x1], #2", 0x4ddf4820);
    check!("ld1 {{v0.h}}[5], [x1], x4", 0x4dc44820);
    check!("ld1 {{v0.h}}[6], [x1]", 0x4d405020);
    check!("ld1 {{v0.h}}[6], [x1], #2", 0x4ddf5020);
    check!("ld1 {{v0.h}}[6], [x1], x4", 0x4dc45020);
    check!("ld1 {{v0.h}}[7], [x1]", 0x4d405820);
    check!("ld1 {{v0.h}}[7], [x1], #2", 0x4ddf5820);
    check!("ld1 {{v0.h}}[7], [x1], x4", 0x4dc45820);
    check!("ld1 {{v0.s}}[0], [x1]", 0x0d408020);
    check!("ld1 {{v0.s}}[0], [x1], #4", 0x0ddf8020);
    check!("ld1 {{v0.s}}[0], [x1], x4", 0x0dc48020);
    check!("ld1 {{v0.s}}[1], [x1]", 0x0d409020);
    check!("ld1 {{v0.s}}[1], [x1], #4", 0x0ddf9020);
    check!("ld1 {{v0.s}}[1], [x1], x4", 0x0dc49020);
    check!("ld1 {{v0.s}}[2], [x1]", 0x4d408020);
    check!("ld1 {{v0.s}}[2], [x1], #4", 0x4ddf8020);
    check!("ld1 {{v0.s}}[2], [x1], x4", 0x4dc48020);
    check!("ld1 {{v0.s}}[3], [x1]", 0x4d409020);
    check!("ld1 {{v0.s}}[3], [x1], #4", 0x4ddf9020);
    check!("ld1 {{v0.s}}[3], [x1], x4", 0x4dc49020);
    check!("ld1 {{v0.d}}[0], [x1]", 0x0d408420);
    check!("ld1 {{v0.d}}[0], [x1], #8", 0x0ddf8420);
    check!("ld1 {{v0.d}}[0], [x1], x4", 0x0dc48420);
    check!("ld1 {{v0.d}}[1], [x1]", 0x4d408420);
    check!("ld1 {{v0.d}}[1], [x1], #8", 0x4ddf8420);
    check!("ld1 {{v0.d}}[1], [x1], x4", 0x4dc48420);
    check!("ld2 {{v0.b, v1.b}}[0], [x1]", 0x0d600020);
    check!("ld2 {{v0.b, v1.b}}[0], [x1], #2", 0x0dff0020);
    check!("ld2 {{v0.b, v1.b}}[0], [x1], x4", 0x0de40020);
    check!("ld2 {{v0.b, v1.b}}[1], [x1]", 0x0d600420);
    check!("ld2 {{v0.b, v1.b}}[1], [x1], #2", 0x0dff0420);
    check!("ld2 {{v0.b, v1.b}}[1], [x1], x4", 0x0de40420);
    check!("ld2 {{v0.b, v1.b}}[2], [x1]", 0x0d600820);
    check!("ld2 {{v0.b, v1.b}}[2], [x1], #2", 0x0dff0820);
    check!("ld2 {{v0.b, v1.b}}[2], [x1], x4", 0x0de40820);
    check!("ld2 {{v0.b, v1.b}}[3], [x1]", 0x0d600c20);
    check!("ld2 {{v0.b, v1.b}}[3], [x1], #2", 0x0dff0c20);
    check!("ld2 {{v0.b, v1.b}}[3], [x1], x4", 0x0de40c20);
    check!("ld2 {{v0.b, v1.b}}[4], [x1]", 0x0d601020);
    check!("ld2 {{v0.b, v1.b}}[4], [x1], #2", 0x0dff1020);
    check!("ld2 {{v0.b, v1.b}}[4], [x1], x4", 0x0de41020);
    check!("ld2 {{v0.b, v1.b}}[5], [x1]", 0x0d601420);
    check!("ld2 {{v0.b, v1.b}}[5], [x1], #2", 0x0dff1420);
    check!("ld2 {{v0.b, v1.b}}[5], [x1], x4", 0x0de41420);
    check!("ld2 {{v0.b, v1.b}}[6], [x1]", 0x0d601820);
    check!("ld2 {{v0.b, v1.b}}[6], [x1], #2", 0x0dff1820);
    check!("ld2 {{v0.b, v1.b}}[6], [x1], x4", 0x0de41820);
    check!("ld2 {{v0.b, v1.b}}[7], [x1]", 0x0d601c20);
    check!("ld2 {{v0.b, v1.b}}[7], [x1], #2", 0x0dff1c20);
    check!("ld2 {{v0.b, v1.b}}[7], [x1], x4", 0x0de41c20);
    check!("ld2 {{v0.b, v1.b}}[8], [x1]", 0x4d600020);
    check!("ld2 {{v0.b, v1.b}}[8], [x1], #2", 0x4dff0020);
    check!("ld2 {{v0.b, v1.b}}[8], [x1], x4", 0x4de40020);
    check!("ld2 {{v0.b, v1.b}}[9], [x1]", 0x4d600420);
    check!("ld2 {{v0.b, v1.b}}[9], [x1], #2", 0x4dff0420);
    check!("ld2 {{v0.b, v1.b}}[9], [x1], x4", 0x4de40420);
    check!("ld2 {{v0.b, v1.b}}[10], [x1]", 0x4d600820);
    check!("ld2 {{v0.b, v1.b}}[10], [x1], #2", 0x4dff0820);
    check!("ld2 {{v0.b, v1.b}}[10], [x1], x4", 0x4de40820);
    check!("ld2 {{v0.b, v1.b}}[11], [x1]", 0x4d600c20);
    check!("ld2 {{v0.b, v1.b}}[11], [x1], #2", 0x4dff0c20);
    check!("ld2 {{v0.b, v1.b}}[11], [x1], x4", 0x4de40c20);
    check!("ld2 {{v0.b, v1.b}}[12], [x1]", 0x4d601020);
    check!("ld2 {{v0.b, v1.b}}[12], [x1], #2", 0x4dff1020);
    check!("ld2 {{v0.b, v1.b}}[12], [x1], x4", 0x4de41020);
    check!("ld2 {{v0.b, v1.b}}[13], [x1]", 0x4d601420);
    check!("ld2 {{v0.b, v1.b}}[13], [x1], #2", 0x4dff1420);
    check!("ld2 {{v0.b, v1.b}}[13], [x1], x4", 0x4de41420);
    check!("ld2 {{v0.b, v1.b}}[14], [x1]", 0x4d601820);
    check!("ld2 {{v0.b, v1.b}}[14], [x1], #2", 0x4dff1820);
    check!("ld2 {{v0.b, v1.b}}[14], [x1], x4", 0x4de41820);
    check!("ld2 {{v0.b, v1.b}}[15], [x1]", 0x4d601c20);
    check!("ld2 {{v0.b, v1.b}}[15], [x1], #2", 0x4dff1c20);
    check!("ld2 {{v0.b, v1.b}}[15], [x1], x4", 0x4de41c20);
    check!("ld2 {{v0.h, v1.h}}[0], [x1]", 0x0d604020);
    check!("ld2 {{v0.h, v1.h}}[0], [x1], #4", 0x0dff4020);
    check!("ld2 {{v0.h, v1.h}}[0], [x1], x4", 0x0de44020);
    check!("ld2 {{v0.h, v1.h}}[1], [x1]", 0x0d604820);
    check!("ld2 {{v0.h, v1.h}}[1], [x1], #4", 0x0dff4820);
    check!("ld2 {{v0.h, v1.h}}[1], [x1], x4", 0x0de44820);
    check!("ld2 {{v0.h, v1.h}}[2], [x1]", 0x0d605020);
    check!("ld2 {{v0.h, v1.h}}[2], [x1], #4", 0x0dff5020);
    check!("ld2 {{v0.h, v1.h}}[2], [x1], x4", 0x0de45020);
    check!("ld2 {{v0.h, v1.h}}[3], [x1]", 0x0d605820);
    check!("ld2 {{v0.h, v1.h}}[3], [x1], #4", 0x0dff5820);
    check!("ld2 {{v0.h, v1.h}}[3], [x1], x4", 0x0de45820);
    check!("ld2 {{v0.h, v1.h}}[4], [x1]", 0x4d604020);
    check!("ld2 {{v0.h, v1.h}}[4], [x1], #4", 0x4dff4020);
    check!("ld2 {{v0.h, v1.h}}[4], [x1], x4", 0x4de44020);
    check!("ld2 {{v0.h, v1.h}}[5], [x1]", 0x4d604820);
    check!("ld2 {{v0.h, v1.h}}[5], [x1], #4", 0x4dff4820);
    check!("ld2 {{v0.h, v1.h}}[5], [x1], x4", 0x4de44820);
    check!("ld2 {{v0.h, v1.h}}[6], [x1]", 0x4d605020);
    check!("ld2 {{v0.h, v1.h}}[6], [x1], #4", 0x4dff5020);
    check!("ld2 {{v0.h, v1.h}}[6], [x1], x4", 0x4de45020);
    check!("ld2 {{v0.h, v1.h}}[7], [x1]", 0x4d605820);
    check!("ld2 {{v0.h, v1.h}}[7], [x1], #4", 0x4dff5820);
    check!("ld2 {{v0.h, v1.h}}[7], [x1], x4", 0x4de45820);
    check!("ld2 {{v0.s, v1.s}}[0], [x1]", 0x0d608020);
    check!("ld2 {{v0.s, v1.s}}[0], [x1], #8", 0x0dff8020);
    check!("ld2 {{v0.s, v1.s}}[0], [x1], x4", 0x0de48020);
    check!("ld2 {{v0.s, v1.s}}[1], [x1]", 0x0d609020);
    check!("ld2 {{v0.s, v1.s}}[1], [x1], #8", 0x0dff9020);
    check!("ld2 {{v0.s, v1.s}}[1], [x1], x4", 0x0de49020);
    check!("ld2 {{v0.s, v1.s}}[2], [x1]", 0x4d608020);
    check!("ld2 {{v0.s, v1.s}}[2], [x1], #8", 0x4dff8020);
    check!("ld2 {{v0.s, v1.s}}[2], [x1], x4", 0x4de48020);
    check!("ld2 {{v0.s, v1.s}}[3], [x1]", 0x4d609020);
    check!("ld2 {{v0.s, v1.s}}[3], [x1], #8", 0x4dff9020);
    check!("ld2 {{v0.s, v1.s}}[3], [x1], x4", 0x4de49020);
    check!("ld2 {{v0.d, v1.d}}[0], [x1]", 0x0d608420);
    check!("ld2 {{v0.d, v1.d}}[0], [x1], #16", 0x0dff8420);
    check!("ld2 {{v0.d, v1.d}}[0], [x1], x4", 0x0de48420);
    check!("ld2 {{v0.d, v1.d}}[1], [x1]", 0x4d608420);
    check!("ld2 {{v0.d, v1.d}}[1], [x1], #16", 0x4dff8420);
    check!("ld2 {{v0.d, v1.d}}[1], [x1], x4", 0x4de48420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[0], [x1]", 0x0d402020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[0], [x1], #3", 0x0ddf2020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[0], [x1], x4", 0x0dc42020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[1], [x1]", 0x0d402420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[1], [x1], #3", 0x0ddf2420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[1], [x1], x4", 0x0dc42420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[2], [x1]", 0x0d402820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[2], [x1], #3", 0x0ddf2820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[2], [x1], x4", 0x0dc42820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[3], [x1]", 0x0d402c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[3], [x1], #3", 0x0ddf2c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[3], [x1], x4", 0x0dc42c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[4], [x1]", 0x0d403020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[4], [x1], #3", 0x0ddf3020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[4], [x1], x4", 0x0dc43020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[5], [x1]", 0x0d403420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[5], [x1], #3", 0x0ddf3420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[5], [x1], x4", 0x0dc43420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[6], [x1]", 0x0d403820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[6], [x1], #3", 0x0ddf3820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[6], [x1], x4", 0x0dc43820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[7], [x1]", 0x0d403c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[7], [x1], #3", 0x0ddf3c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[7], [x1], x4", 0x0dc43c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[8], [x1]", 0x4d402020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[8], [x1], #3", 0x4ddf2020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[8], [x1], x4", 0x4dc42020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[9], [x1]", 0x4d402420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[9], [x1], #3", 0x4ddf2420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[9], [x1], x4", 0x4dc42420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[10], [x1]", 0x4d402820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[10], [x1], #3", 0x4ddf2820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[10], [x1], x4", 0x4dc42820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[11], [x1]", 0x4d402c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[11], [x1], #3", 0x4ddf2c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[11], [x1], x4", 0x4dc42c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[12], [x1]", 0x4d403020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[12], [x1], #3", 0x4ddf3020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[12], [x1], x4", 0x4dc43020);
    check!("ld3 {{v0.b, v1.b, v2.b}}[13], [x1]", 0x4d403420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[13], [x1], #3", 0x4ddf3420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[13], [x1], x4", 0x4dc43420);
    check!("ld3 {{v0.b, v1.b, v2.b}}[14], [x1]", 0x4d403820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[14], [x1], #3", 0x4ddf3820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[14], [x1], x4", 0x4dc43820);
    check!("ld3 {{v0.b, v1.b, v2.b}}[15], [x1]", 0x4d403c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[15], [x1], #3", 0x4ddf3c20);
    check!("ld3 {{v0.b, v1.b, v2.b}}[15], [x1], x4", 0x4dc43c20);
    check!("ld3 {{v0.h, v1.h, v2.h}}[0], [x1]", 0x0d406020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[0], [x1], #6", 0x0ddf6020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[0], [x1], x4", 0x0dc46020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[1], [x1]", 0x0d406820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[1], [x1], #6", 0x0ddf6820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[1], [x1], x4", 0x0dc46820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[2], [x1]", 0x0d407020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[2], [x1], #6", 0x0ddf7020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[2], [x1], x4", 0x0dc47020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[3], [x1]", 0x0d407820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[3], [x1], #6", 0x0ddf7820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[3], [x1], x4", 0x0dc47820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[4], [x1]", 0x4d406020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[4], [x1], #6", 0x4ddf6020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[4], [x1], x4", 0x4dc46020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[5], [x1]", 0x4d406820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[5], [x1], #6", 0x4ddf6820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[5], [x1], x4", 0x4dc46820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[6], [x1]", 0x4d407020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[6], [x1], #6", 0x4ddf7020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[6], [x1], x4", 0x4dc47020);
    check!("ld3 {{v0.h, v1.h, v2.h}}[7], [x1]", 0x4d407820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[7], [x1], #6", 0x4ddf7820);
    check!("ld3 {{v0.h, v1.h, v2.h}}[7], [x1], x4", 0x4dc47820);
    check!("ld3 {{v0.s, v1.s, v2.s}}[0], [x1]", 0x0d40a020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[0], [x1], #12", 0x0ddfa020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[0], [x1], x4", 0x0dc4a020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[1], [x1]", 0x0d40b020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[1], [x1], #12", 0x0ddfb020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[1], [x1], x4", 0x0dc4b020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[2], [x1]", 0x4d40a020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[2], [x1], #12", 0x4ddfa020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[2], [x1], x4", 0x4dc4a020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[3], [x1]", 0x4d40b020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[3], [x1], #12", 0x4ddfb020);
    check!("ld3 {{v0.s, v1.s, v2.s}}[3], [x1], x4", 0x4dc4b020);
    check!("ld3 {{v0.d, v1.d, v2.d}}[0], [x1]", 0x0d40a420);
    check!("ld3 {{v0.d, v1.d, v2.d}}[0], [x1], #24", 0x0ddfa420);
    check!("ld3 {{v0.d, v1.d, v2.d}}[0], [x1], x4", 0x0dc4a420);
    check!("ld3 {{v0.d, v1.d, v2.d}}[1], [x1]", 0x4d40a420);
    check!("ld3 {{v0.d, v1.d, v2.d}}[1], [x1], #24", 0x4ddfa420);
    check!("ld3 {{v0.d, v1.d, v2.d}}[1], [x1], x4", 0x4dc4a420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[0], [x1]", 0x0d602020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[0], [x1], #4", 0x0dff2020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[0], [x1], x4", 0x0de42020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[1], [x1]", 0x0d602420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[1], [x1], #4", 0x0dff2420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[1], [x1], x4", 0x0de42420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[2], [x1]", 0x0d602820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[2], [x1], #4", 0x0dff2820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[2], [x1], x4", 0x0de42820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[3], [x1]", 0x0d602c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[3], [x1], #4", 0x0dff2c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[3], [x1], x4", 0x0de42c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[4], [x1]", 0x0d603020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[4], [x1], #4", 0x0dff3020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[4], [x1], x4", 0x0de43020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[5], [x1]", 0x0d603420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[5], [x1], #4", 0x0dff3420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[5], [x1], x4", 0x0de43420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[6], [x1]", 0x0d603820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[6], [x1], #4", 0x0dff3820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[6], [x1], x4", 0x0de43820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[7], [x1]", 0x0d603c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[7], [x1], #4", 0x0dff3c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[7], [x1], x4", 0x0de43c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[8], [x1]", 0x4d602020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[8], [x1], #4", 0x4dff2020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[8], [x1], x4", 0x4de42020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[9], [x1]", 0x4d602420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[9], [x1], #4", 0x4dff2420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[9], [x1], x4", 0x4de42420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[10], [x1]", 0x4d602820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[10], [x1], #4", 0x4dff2820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[10], [x1], x4", 0x4de42820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[11], [x1]", 0x4d602c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[11], [x1], #4", 0x4dff2c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[11], [x1], x4", 0x4de42c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[12], [x1]", 0x4d603020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[12], [x1], #4", 0x4dff3020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[12], [x1], x4", 0x4de43020);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[13], [x1]", 0x4d603420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[13], [x1], #4", 0x4dff3420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[13], [x1], x4", 0x4de43420);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[14], [x1]", 0x4d603820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[14], [x1], #4", 0x4dff3820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[14], [x1], x4", 0x4de43820);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[15], [x1]", 0x4d603c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[15], [x1], #4", 0x4dff3c20);
    check!("ld4 {{v0.b, v1.b, v2.b, v3.b}}[15], [x1], x4", 0x4de43c20);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[0], [x1]", 0x0d606020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[0], [x1], #8", 0x0dff6020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[0], [x1], x4", 0x0de46020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[1], [x1]", 0x0d606820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[1], [x1], #8", 0x0dff6820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[1], [x1], x4", 0x0de46820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[2], [x1]", 0x0d607020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[2], [x1], #8", 0x0dff7020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[2], [x1], x4", 0x0de47020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[3], [x1]", 0x0d607820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[3], [x1], #8", 0x0dff7820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[3], [x1], x4", 0x0de47820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[4], [x1]", 0x4d606020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[4], [x1], #8", 0x4dff6020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[4], [x1], x4", 0x4de46020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[5], [x1]", 0x4d606820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[5], [x1], #8", 0x4dff6820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[5], [x1], x4", 0x4de46820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[6], [x1]", 0x4d607020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[6], [x1], #8", 0x4dff7020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[6], [x1], x4", 0x4de47020);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[7], [x1]", 0x4d607820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[7], [x1], #8", 0x4dff7820);
    check!("ld4 {{v0.h, v1.h, v2.h, v3.h}}[7], [x1], x4", 0x4de47820);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[0], [x1]", 0x0d60a020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[0], [x1], #16", 0x0dffa020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[0], [x1], x4", 0x0de4a020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[1], [x1]", 0x0d60b020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[1], [x1], #16", 0x0dffb020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[1], [x1], x4", 0x0de4b020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[2], [x1]", 0x4d60a020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[2], [x1], #16", 0x4dffa020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[2], [x1], x4", 0x4de4a020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[3], [x1]", 0x4d60b020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[3], [x1], #16", 0x4dffb020);
    check!("ld4 {{v0.s, v1.s, v2.s, v3.s}}[3], [x1], x4", 0x4de4b020);
    check!("ld4 {{v0.d, v1.d, v2.d, v3.d}}[0], [x1]", 0x0d60a420);
    check!("ld4 {{v0.d, v1.d, v2.d, v3.d}}[0], [x1], #32", 0x0dffa420);
    check!("ld4 {{v0.d, v1.d, v2.d, v3.d}}[0], [x1], x4", 0x0de4a420);
    check!("ld4 {{v0.d, v1.d, v2.d, v3.d}}[1], [x1]", 0x4d60a420);
    check!("ld4 {{v0.d, v1.d, v2.d, v3.d}}[1], [x1], #32", 0x4dffa420);
    check!("ld4 {{v0.d, v1.d, v2.d, v3.d}}[1], [x1], x4", 0x4de4a420);
    check!("st1 {{v0.b}}[0], [x1]", 0x0d000020);
    check!("st1 {{v0.b}}[0], [x1], #1", 0x0d9f0020);
    check!("st1 {{v0.b}}[0], [x1], x4", 0x0d840020);
    check!("st1 {{v0.b}}[1], [x1]", 0x0d000420);
    check!("st1 {{v0.b}}[1], [x1], #1", 0x0d9f0420);
    check!("st1 {{v0.b}}[1], [x1], x4", 0x0d840420);
    check!("st1 {{v0.b}}[2], [x1]", 0x0d000820);
    check!("st1 {{v0.b}}[2], [x1], #1", 0x0d9f0820);
    check!("st1 {{v0.b}}[2], [x1], x4", 0x0d840820);
    check!("st1 {{v0.b}}[3], [x1]", 0x0d000c20);
    check!("st1 {{v0.b}}[3], [x1], #1", 0x0d9f0c20);
    check!("st1 {{v0.b}}[3], [x1], x4", 0x0d840c20);
    check!("st1 {{v0.b}}[4], [x1]", 0x0d001020);
    check!("st1 {{v0.b}}[4], [x1], #1", 0x0d9f1020);
    check!("st1 {{v0.b}}[4], [x1], x4", 0x0d841020);
    check!("st1 {{v0.b}}[5], [x1]", 0x0d001420);
    check!("st1 {{v0.b}}[5], [x1], #1", 0x0d9f1420);
    check!("st1 {{v0.b}}[5], [x1], x4", 0x0d841420);
    check!("st1 {{v0.b}}[6], [x1]", 0x0d001820);
    check!("st1 {{v0.b}}[6], [x1], #1", 0x0d9f1820);
    check!("st1 {{v0.b}}[6], [x1], x4", 0x0d841820);
    check!("st1 {{v0.b}}[7], [x1]", 0x0d001c20);
    check!("st1 {{v0.b}}[7], [x1], #1", 0x0d9f1c20);
    check!("st1 {{v0.b}}[7], [x1], x4", 0x0d841c20);
    check!("st1 {{v0.b}}[8], [x1]", 0x4d000020);
    check!("st1 {{v0.b}}[8], [x1], #1", 0x4d9f0020);
    check!("st1 {{v0.b}}[8], [x1], x4", 0x4d840020);
    check!("st1 {{v0.b}}[9], [x1]", 0x4d000420);
    check!("st1 {{v0.b}}[9], [x1], #1", 0x4d9f0420);
    check!("st1 {{v0.b}}[9], [x1], x4", 0x4d840420);
    check!("st1 {{v0.b}}[10], [x1]", 0x4d000820);
    check!("st1 {{v0.b}}[10], [x1], #1", 0x4d9f0820);
    check!("st1 {{v0.b}}[10], [x1], x4", 0x4d840820);
    check!("st1 {{v0.b}}[11], [x1]", 0x4d000c20);
    check!("st1 {{v0.b}}[11], [x1], #1", 0x4d9f0c20);
    check!("st1 {{v0.b}}[11], [x1], x4", 0x4d840c20);
    check!("st1 {{v0.b}}[12], [x1]", 0x4d001020);
    check!("st1 {{v0.b}}[12], [x1], #1", 0x4d9f1020);
    check!("st1 {{v0.b}}[12], [x1], x4", 0x4d841020);
    check!("st1 {{v0.b}}[13], [x1]", 0x4d001420);
    check!("st1 {{v0.b}}[13], [x1], #1", 0x4d9f1420);
    check!("st1 {{v0.b}}[13], [x1], x4", 0x4d841420);
    check!("st1 {{v0.b}}[14], [x1]", 0x4d001820);
    check!("st1 {{v0.b}}[14], [x1], #1", 0x4d9f1820);
    check!("st1 {{v0.b}}[14], [x1], x4", 0x4d841820);
    check!("st1 {{v0.b}}[15], [x1]", 0x4d001c20);
    check!("st1 {{v0.b}}[15], [x1], #1", 0x4d9f1c20);
    check!("st1 {{v0.b}}[15], [x1], x4", 0x4d841c20);
    check!("st1 {{v0.h}}[0], [x1]", 0x0d004020);
    check!("st1 {{v0.h}}[0], [x1], #2", 0x0d9f4020);
    check!("st1 {{v0.h}}[0], [x1], x4", 0x0d844020);
    check!("st1 {{v0.h}}[1], [x1]", 0x0d004820);
    check!("st1 {{v0.h}}[1], [x1], #2", 0x0d9f4820);
    check!("st1 {{v0.h}}[1], [x1], x4", 0x0d844820);
    check!("st1 {{v0.h}}[2], [x1]", 0x0d005020);
    check!("st1 {{v0.h}}[2], [x1], #2", 0x0d9f5020);
    check!("st1 {{v0.h}}[2], [x1], x4", 0x0d845020);
    check!("st1 {{v0.h}}[3], [x1]", 0x0d005820);
    check!("st1 {{v0.h}}[3], [x1], #2", 0x0d9f5820);
    check!("st1 {{v0.h}}[3], [x1], x4", 0x0d845820);
    check!("st1 {{v0.h}}[4], [x1]", 0x4d004020);
    check!("st1 {{v0.h}}[4], [x1], #2", 0x4d9f4020);
    check!("st1 {{v0.h}}[4], [x1], x4", 0x4d844020);
    check!("st1 {{v0.h}}[5], [x1]", 0x4d004820);
    check!("st1 {{v0.h}}[5], [x1], #2", 0x4d9f4820);
    check!("st1 {{v0.h}}[5], [x1], x4", 0x4d844820);
    check!("st1 {{v0.h}}[6], [x1]", 0x4d005020);
    check!("st1 {{v0.h}}[6], [x1], #2", 0x4d9f5020);
    check!("st1 {{v0.h}}[6], [x1], x4", 0x4d845020);
    check!("st1 {{v0.h}}[7], [x1]", 0x4d005820);
    check!("st1 {{v0.h}}[7], [x1], #2", 0x4d9f5820);
    check!("st1 {{v0.h}}[7], [x1], x4", 0x4d845820);
    check!("st1 {{v0.s}}[0], [x1]", 0x0d008020);
    check!("st1 {{v0.s}}[0], [x1], #4", 0x0d9f8020);
    check!("st1 {{v0.s}}[0], [x1], x4", 0x0d848020);
    check!("st1 {{v0.s}}[1], [x1]", 0x0d009020);
    check!("st1 {{v0.s}}[1], [x1], #4", 0x0d9f9020);
    check!("st1 {{v0.s}}[1], [x1], x4", 0x0d849020);
    check!("st1 {{v0.s}}[2], [x1]", 0x4d008020);
    check!("st1 {{v0.s}}[2], [x1], #4", 0x4d9f8020);
    check!("st1 {{v0.s}}[2], [x1], x4", 0x4d848020);
    check!("st1 {{v0.s}}[3], [x1]", 0x4d009020);
    check!("st1 {{v0.s}}[3], [x1], #4", 0x4d9f9020);
    check!("st1 {{v0.s}}[3], [x1], x4", 0x4d849020);
    check!("st1 {{v0.d}}[0], [x1]", 0x0d008420);
    check!("st1 {{v0.d}}[0], [x1], #8", 0x0d9f8420);
    check!("st1 {{v0.d}}[0], [x1], x4", 0x0d848420);
    check!("st1 {{v0.d}}[1], [x1]", 0x4d008420);
    check!("st1 {{v0.d}}[1], [x1], #8", 0x4d9f8420);
    check!("st1 {{v0.d}}[1], [x1], x4", 0x4d848420);
    check!("st2 {{v0.b, v1.b}}[0], [x1]", 0x0d200020);
    check!("st2 {{v0.b, v1.b}}[0], [x1], #2", 0x0dbf0020);
    check!("st2 {{v0.b, v1.b}}[0], [x1], x4", 0x0da40020);
    check!("st2 {{v0.b, v1.b}}[1], [x1]", 0x0d200420);
    check!("st2 {{v0.b, v1.b}}[1], [x1], #2", 0x0dbf0420);
    check!("st2 {{v0.b, v1.b}}[1], [x1], x4", 0x0da40420);
    check!("st2 {{v0.b, v1.b}}[2], [x1]", 0x0d200820);
    check!("st2 {{v0.b, v1.b}}[2], [x1], #2", 0x0dbf0820);
    check!("st2 {{v0.b, v1.b}}[2], [x1], x4", 0x0da40820);
    check!("st2 {{v0.b, v1.b}}[3], [x1]", 0x0d200c20);
    check!("st2 {{v0.b, v1.b}}[3], [x1], #2", 0x0dbf0c20);
    check!("st2 {{v0.b, v1.b}}[3], [x1], x4", 0x0da40c20);
    check!("st2 {{v0.b, v1.b}}[4], [x1]", 0x0d201020);
    check!("st2 {{v0.b, v1.b}}[4], [x1], #2", 0x0dbf1020);
    check!("st2 {{v0.b, v1.b}}[4], [x1], x4", 0x0da41020);
    check!("st2 {{v0.b, v1.b}}[5], [x1]", 0x0d201420);
    check!("st2 {{v0.b, v1.b}}[5], [x1], #2", 0x0dbf1420);
    check!("st2 {{v0.b, v1.b}}[5], [x1], x4", 0x0da41420);
    check!("st2 {{v0.b, v1.b}}[6], [x1]", 0x0d201820);
    check!("st2 {{v0.b, v1.b}}[6], [x1], #2", 0x0dbf1820);
    check!("st2 {{v0.b, v1.b}}[6], [x1], x4", 0x0da41820);
    check!("st2 {{v0.b, v1.b}}[7], [x1]", 0x0d201c20);
    check!("st2 {{v0.b, v1.b}}[7], [x1], #2", 0x0dbf1c20);
    check!("st2 {{v0.b, v1.b}}[7], [x1], x4", 0x0da41c20);
    check!("st2 {{v0.b, v1.b}}[8], [x1]", 0x4d200020);
    check!("st2 {{v0.b, v1.b}}[8], [x1], #2", 0x4dbf0020);
    check!("st2 {{v0.b, v1.b}}[8], [x1], x4", 0x4da40020);
    check!("st2 {{v0.b, v1.b}}[9], [x1]", 0x4d200420);
    check!("st2 {{v0.b, v1.b}}[9], [x1], #2", 0x4dbf0420);
    check!("st2 {{v0.b, v1.b}}[9], [x1], x4", 0x4da40420);
    check!("st2 {{v0.b, v1.b}}[10], [x1]", 0x4d200820);
    check!("st2 {{v0.b, v1.b}}[10], [x1], #2", 0x4dbf0820);
    check!("st2 {{v0.b, v1.b}}[10], [x1], x4", 0x4da40820);
    check!("st2 {{v0.b, v1.b}}[11], [x1]", 0x4d200c20);
    check!("st2 {{v0.b, v1.b}}[11], [x1], #2", 0x4dbf0c20);
    check!("st2 {{v0.b, v1.b}}[11], [x1], x4", 0x4da40c20);
    check!("st2 {{v0.b, v1.b}}[12], [x1]", 0x4d201020);
    check!("st2 {{v0.b, v1.b}}[12], [x1], #2", 0x4dbf1020);
    check!("st2 {{v0.b, v1.b}}[12], [x1], x4", 0x4da41020);
    check!("st2 {{v0.b, v1.b}}[13], [x1]", 0x4d201420);
    check!("st2 {{v0.b, v1.b}}[13], [x1], #2", 0x4dbf1420);
    check!("st2 {{v0.b, v1.b}}[13], [x1], x4", 0x4da41420);
    check!("st2 {{v0.b, v1.b}}[14], [x1]", 0x4d201820);
    check!("st2 {{v0.b, v1.b}}[14], [x1], #2", 0x4dbf1820);
    check!("st2 {{v0.b, v1.b}}[14], [x1], x4", 0x4da41820);
    check!("st2 {{v0.b, v1.b}}[15], [x1]", 0x4d201c20);
    check!("st2 {{v0.b, v1.b}}[15], [x1], #2", 0x4dbf1c20);
    check!("st2 {{v0.b, v1.b}}[15], [x1], x4", 0x4da41c20);
    check!("st2 {{v0.h, v1.h}}[0], [x1]", 0x0d204020);
    check!("st2 {{v0.h, v1.h}}[0], [x1], #4", 0x0dbf4020);
    check!("st2 {{v0.h, v1.h}}[0], [x1], x4", 0x0da44020);
    check!("st2 {{v0.h, v1.h}}[1], [x1]", 0x0d204820);
    check!("st2 {{v0.h, v1.h}}[1], [x1], #4", 0x0dbf4820);
    check!("st2 {{v0.h, v1.h}}[1], [x1], x4", 0x0da44820);
    check!("st2 {{v0.h, v1.h}}[2], [x1]", 0x0d205020);
    check!("st2 {{v0.h, v1.h}}[2], [x1], #4", 0x0dbf5020);
    check!("st2 {{v0.h, v1.h}}[2], [x1], x4", 0x0da45020);
    check!("st2 {{v0.h, v1.h}}[3], [x1]", 0x0d205820);
    check!("st2 {{v0.h, v1.h}}[3], [x1], #4", 0x0dbf5820);
    check!("st2 {{v0.h, v1.h}}[3], [x1], x4", 0x0da45820);
    check!("st2 {{v0.h, v1.h}}[4], [x1]", 0x4d204020);
    check!("st2 {{v0.h, v1.h}}[4], [x1], #4", 0x4dbf4020);
    check!("st2 {{v0.h, v1.h}}[4], [x1], x4", 0x4da44020);
    check!("st2 {{v0.h, v1.h}}[5], [x1]", 0x4d204820);
    check!("st2 {{v0.h, v1.h}}[5], [x1], #4", 0x4dbf4820);
    check!("st2 {{v0.h, v1.h}}[5], [x1], x4", 0x4da44820);
    check!("st2 {{v0.h, v1.h}}[6], [x1]", 0x4d205020);
    check!("st2 {{v0.h, v1.h}}[6], [x1], #4", 0x4dbf5020);
    check!("st2 {{v0.h, v1.h}}[6], [x1], x4", 0x4da45020);
    check!("st2 {{v0.h, v1.h}}[7], [x1]", 0x4d205820);
    check!("st2 {{v0.h, v1.h}}[7], [x1], #4", 0x4dbf5820);
    check!("st2 {{v0.h, v1.h}}[7], [x1], x4", 0x4da45820);
    check!("st2 {{v0.s, v1.s}}[0], [x1]", 0x0d208020);
    check!("st2 {{v0.s, v1.s}}[0], [x1], #8", 0x0dbf8020);
    check!("st2 {{v0.s, v1.s}}[0], [x1], x4", 0x0da48020);
    check!("st2 {{v0.s, v1.s}}[1], [x1]", 0x0d209020);
    check!("st2 {{v0.s, v1.s}}[1], [x1], #8", 0x0dbf9020);
    check!("st2 {{v0.s, v1.s}}[1], [x1], x4", 0x0da49020);
    check!("st2 {{v0.s, v1.s}}[2], [x1]", 0x4d208020);
    check!("st2 {{v0.s, v1.s}}[2], [x1], #8", 0x4dbf8020);
    check!("st2 {{v0.s, v1.s}}[2], [x1], x4", 0x4da48020);
    check!("st2 {{v0.s, v1.s}}[3], [x1]", 0x4d209020);
    check!("st2 {{v0.s, v1.s}}[3], [x1], #8", 0x4dbf9020);
    check!("st2 {{v0.s, v1.s}}[3], [x1], x4", 0x4da49020);
    check!("st2 {{v0.d, v1.d}}[0], [x1]", 0x0d208420);
    check!("st2 {{v0.d, v1.d}}[0], [x1], #16", 0x0dbf8420);
    check!("st2 {{v0.d, v1.d}}[0], [x1], x4", 0x0da48420);
    check!("st2 {{v0.d, v1.d}}[1], [x1]", 0x4d208420);
    check!("st2 {{v0.d, v1.d}}[1], [x1], #16", 0x4dbf8420);
    check!("st2 {{v0.d, v1.d}}[1], [x1], x4", 0x4da48420);
    check!("st3 {{v0.b, v1.b, v2.b}}[0], [x1]", 0x0d002020);
    check!("st3 {{v0.b, v1.b, v2.b}}[0], [x1], #3", 0x0d9f2020);
    check!("st3 {{v0.b, v1.b, v2.b}}[0], [x1], x4", 0x0d842020);
    check!("st3 {{v0.b, v1.b, v2.b}}[1], [x1]", 0x0d002420);
    check!("st3 {{v0.b, v1.b, v2.b}}[1], [x1], #3", 0x0d9f2420);
    check!("st3 {{v0.b, v1.b, v2.b}}[1], [x1], x4", 0x0d842420);
    check!("st3 {{v0.b, v1.b, v2.b}}[2], [x1]", 0x0d002820);
    check!("st3 {{v0.b, v1.b, v2.b}}[2], [x1], #3", 0x0d9f2820);
    check!("st3 {{v0.b, v1.b, v2.b}}[2], [x1], x4", 0x0d842820);
    check!("st3 {{v0.b, v1.b, v2.b}}[3], [x1]", 0x0d002c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[3], [x1], #3", 0x0d9f2c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[3], [x1], x4", 0x0d842c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[4], [x1]", 0x0d003020);
    check!("st3 {{v0.b, v1.b, v2.b}}[4], [x1], #3", 0x0d9f3020);
    check!("st3 {{v0.b, v1.b, v2.b}}[4], [x1], x4", 0x0d843020);
    check!("st3 {{v0.b, v1.b, v2.b}}[5], [x1]", 0x0d003420);
    check!("st3 {{v0.b, v1.b, v2.b}}[5], [x1], #3", 0x0d9f3420);
    check!("st3 {{v0.b, v1.b, v2.b}}[5], [x1], x4", 0x0d843420);
    check!("st3 {{v0.b, v1.b, v2.b}}[6], [x1]", 0x0d003820);
    check!("st3 {{v0.b, v1.b, v2.b}}[6], [x1], #3", 0x0d9f3820);
    check!("st3 {{v0.b, v1.b, v2.b}}[6], [x1], x4", 0x0d843820);
    check!("st3 {{v0.b, v1.b, v2.b}}[7], [x1]", 0x0d003c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[7], [x1], #3", 0x0d9f3c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[7], [x1], x4", 0x0d843c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[8], [x1]", 0x4d002020);
    check!("st3 {{v0.b, v1.b, v2.b}}[8], [x1], #3", 0x4d9f2020);
    check!("st3 {{v0.b, v1.b, v2.b}}[8], [x1], x4", 0x4d842020);
    check!("st3 {{v0.b, v1.b, v2.b}}[9], [x1]", 0x4d002420);
    check!("st3 {{v0.b, v1.b, v2.b}}[9], [x1], #3", 0x4d9f2420);
    check!("st3 {{v0.b, v1.b, v2.b}}[9], [x1], x4", 0x4d842420);
    check!("st3 {{v0.b, v1.b, v2.b}}[10], [x1]", 0x4d002820);
    check!("st3 {{v0.b, v1.b, v2.b}}[10], [x1], #3", 0x4d9f2820);
    check!("st3 {{v0.b, v1.b, v2.b}}[10], [x1], x4", 0x4d842820);
    check!("st3 {{v0.b, v1.b, v2.b}}[11], [x1]", 0x4d002c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[11], [x1], #3", 0x4d9f2c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[11], [x1], x4", 0x4d842c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[12], [x1]", 0x4d003020);
    check!("st3 {{v0.b, v1.b, v2.b}}[12], [x1], #3", 0x4d9f3020);
    check!("st3 {{v0.b, v1.b, v2.b}}[12], [x1], x4", 0x4d843020);
    check!("st3 {{v0.b, v1.b, v2.b}}[13], [x1]", 0x4d003420);
    check!("st3 {{v0.b, v1.b, v2.b}}[13], [x1], #3", 0x4d9f3420);
    check!("st3 {{v0.b, v1.b, v2.b}}[13], [x1], x4", 0x4d843420);
    check!("st3 {{v0.b, v1.b, v2.b}}[14], [x1]", 0x4d003820);
    check!("st3 {{v0.b, v1.b, v2.b}}[14], [x1], #3", 0x4d9f3820);
    check!("st3 {{v0.b, v1.b, v2.b}}[14], [x1], x4", 0x4d843820);
    check!("st3 {{v0.b, v1.b, v2.b}}[15], [x1]", 0x4d003c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[15], [x1], #3", 0x4d9f3c20);
    check!("st3 {{v0.b, v1.b, v2.b}}[15], [x1], x4", 0x4d843c20);
    check!("st3 {{v0.h, v1.h, v2.h}}[0], [x1]", 0x0d006020);
    check!("st3 {{v0.h, v1.h, v2.h}}[0], [x1], #6", 0x0d9f6020);
    check!("st3 {{v0.h, v1.h, v2.h}}[0], [x1], x4", 0x0d846020);
    check!("st3 {{v0.h, v1.h, v2.h}}[1], [x1]", 0x0d006820);
    check!("st3 {{v0.h, v1.h, v2.h}}[1], [x1], #6", 0x0d9f6820);
    check!("st3 {{v0.h, v1.h, v2.h}}[1], [x1], x4", 0x0d846820);
    check!("st3 {{v0.h, v1.h, v2.h}}[2], [x1]", 0x0d007020);
    check!("st3 {{v0.h, v1.h, v2.h}}[2], [x1], #6", 0x0d9f7020);
    check!("st3 {{v0.h, v1.h, v2.h}}[2], [x1], x4", 0x0d847020);
    check!("st3 {{v0.h, v1.h, v2.h}}[3], [x1]", 0x0d007820);
    check!("st3 {{v0.h, v1.h, v2.h}}[3], [x1], #6", 0x0d9f7820);
    check!("st3 {{v0.h, v1.h, v2.h}}[3], [x1], x4", 0x0d847820);
    check!("st3 {{v0.h, v1.h, v2.h}}[4], [x1]", 0x4d006020);
    check!("st3 {{v0.h, v1.h, v2.h}}[4], [x1], #6", 0x4d9f6020);
    check!("st3 {{v0.h, v1.h, v2.h}}[4], [x1], x4", 0x4d846020);
    check!("st3 {{v0.h, v1.h, v2.h}}[5], [x1]", 0x4d006820);
    check!("st3 {{v0.h, v1.h, v2.h}}[5], [x1], #6", 0x4d9f6820);
    check!("st3 {{v0.h, v1.h, v2.h}}[5], [x1], x4", 0x4d846820);
    check!("st3 {{v0.h, v1.h, v2.h}}[6], [x1]", 0x4d007020);
    check!("st3 {{v0.h, v1.h, v2.h}}[6], [x1], #6", 0x4d9f7020);
    check!("st3 {{v0.h, v1.h, v2.h}}[6], [x1], x4", 0x4d847020);
    check!("st3 {{v0.h, v1.h, v2.h}}[7], [x1]", 0x4d007820);
    check!("st3 {{v0.h, v1.h, v2.h}}[7], [x1], #6", 0x4d9f7820);
    check!("st3 {{v0.h, v1.h, v2.h}}[7], [x1], x4", 0x4d847820);
    check!("st3 {{v0.s, v1.s, v2.s}}[0], [x1]", 0x0d00a020);
    check!("st3 {{v0.s, v1.s, v2.s}}[0], [x1], #12", 0x0d9fa020);
    check!("st3 {{v0.s, v1.s, v2.s}}[0], [x1], x4", 0x0d84a020);
    check!("st3 {{v0.s, v1.s, v2.s}}[1], [x1]", 0x0d00b020);
    check!("st3 {{v0.s, v1.s, v2.s}}[1], [x1], #12", 0x0d9fb020);
    check!("st3 {{v0.s, v1.s, v2.s}}[1], [x1], x4", 0x0d84b020);
    check!("st3 {{v0.s, v1.s, v2.s}}[2], [x1]", 0x4d00a020);
    check!("st3 {{v0.s, v1.s, v2.s}}[2], [x1], #12", 0x4d9fa020);
    check!("st3 {{v0.s, v1.s, v2.s}}[2], [x1], x4", 0x4d84a020);
    check!("st3 {{v0.s, v1.s, v2.s}}[3], [x1]", 0x4d00b020);
    check!("st3 {{v0.s, v1.s, v2.s}}[3], [x1], #12", 0x4d9fb020);
    check!("st3 {{v0.s, v1.s, v2.s}}[3], [x1], x4", 0x4d84b020);
    check!("st3 {{v0.d, v1.d, v2.d}}[0], [x1]", 0x0d00a420);
    check!("st3 {{v0.d, v1.d, v2.d}}[0], [x1], #24", 0x0d9fa420);
    check!("st3 {{v0.d, v1.d, v2.d}}[0], [x1], x4", 0x0d84a420);
    check!("st3 {{v0.d, v1.d, v2.d}}[1], [x1]", 0x4d00a420);
    check!("st3 {{v0.d, v1.d, v2.d}}[1], [x1], #24", 0x4d9fa420);
    check!("st3 {{v0.d, v1.d, v2.d}}[1], [x1], x4", 0x4d84a420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[0], [x1]", 0x0d202020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[0], [x1], #4", 0x0dbf2020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[0], [x1], x4", 0x0da42020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[1], [x1]", 0x0d202420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[1], [x1], #4", 0x0dbf2420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[1], [x1], x4", 0x0da42420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[2], [x1]", 0x0d202820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[2], [x1], #4", 0x0dbf2820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[2], [x1], x4", 0x0da42820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[3], [x1]", 0x0d202c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[3], [x1], #4", 0x0dbf2c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[3], [x1], x4", 0x0da42c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[4], [x1]", 0x0d203020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[4], [x1], #4", 0x0dbf3020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[4], [x1], x4", 0x0da43020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[5], [x1]", 0x0d203420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[5], [x1], #4", 0x0dbf3420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[5], [x1], x4", 0x0da43420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[6], [x1]", 0x0d203820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[6], [x1], #4", 0x0dbf3820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[6], [x1], x4", 0x0da43820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[7], [x1]", 0x0d203c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[7], [x1], #4", 0x0dbf3c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[7], [x1], x4", 0x0da43c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[8], [x1]", 0x4d202020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[8], [x1], #4", 0x4dbf2020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[8], [x1], x4", 0x4da42020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[9], [x1]", 0x4d202420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[9], [x1], #4", 0x4dbf2420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[9], [x1], x4", 0x4da42420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[10], [x1]", 0x4d202820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[10], [x1], #4", 0x4dbf2820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[10], [x1], x4", 0x4da42820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[11], [x1]", 0x4d202c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[11], [x1], #4", 0x4dbf2c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[11], [x1], x4", 0x4da42c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[12], [x1]", 0x4d203020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[12], [x1], #4", 0x4dbf3020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[12], [x1], x4", 0x4da43020);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[13], [x1]", 0x4d203420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[13], [x1], #4", 0x4dbf3420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[13], [x1], x4", 0x4da43420);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[14], [x1]", 0x4d203820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[14], [x1], #4", 0x4dbf3820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[14], [x1], x4", 0x4da43820);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[15], [x1]", 0x4d203c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[15], [x1], #4", 0x4dbf3c20);
    check!("st4 {{v0.b, v1.b, v2.b, v3.b}}[15], [x1], x4", 0x4da43c20);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[0], [x1]", 0x0d206020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[0], [x1], #8", 0x0dbf6020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[0], [x1], x4", 0x0da46020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[1], [x1]", 0x0d206820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[1], [x1], #8", 0x0dbf6820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[1], [x1], x4", 0x0da46820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[2], [x1]", 0x0d207020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[2], [x1], #8", 0x0dbf7020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[2], [x1], x4", 0x0da47020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[3], [x1]", 0x0d207820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[3], [x1], #8", 0x0dbf7820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[3], [x1], x4", 0x0da47820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[4], [x1]", 0x4d206020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[4], [x1], #8", 0x4dbf6020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[4], [x1], x4", 0x4da46020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[5], [x1]", 0x4d206820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[5], [x1], #8", 0x4dbf6820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[5], [x1], x4", 0x4da46820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[6], [x1]", 0x4d207020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[6], [x1], #8", 0x4dbf7020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[6], [x1], x4", 0x4da47020);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[7], [x1]", 0x4d207820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[7], [x1], #8", 0x4dbf7820);
    check!("st4 {{v0.h, v1.h, v2.h, v3.h}}[7], [x1], x4", 0x4da47820);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[0], [x1]", 0x0d20a020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[0], [x1], #16", 0x0dbfa020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[0], [x1], x4", 0x0da4a020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[1], [x1]", 0x0d20b020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[1], [x1], #16", 0x0dbfb020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[1], [x1], x4", 0x0da4b020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[2], [x1]", 0x4d20a020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[2], [x1], #16", 0x4dbfa020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[2], [x1], x4", 0x4da4a020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[3], [x1]", 0x4d20b020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[3], [x1], #16", 0x4dbfb020);
    check!("st4 {{v0.s, v1.s, v2.s, v3.s}}[3], [x1], x4", 0x4da4b020);
    check!("st4 {{v0.d, v1.d, v2.d, v3.d}}[0], [x1]", 0x0d20a420);
    check!("st4 {{v0.d, v1.d, v2.d, v3.d}}[0], [x1], #32", 0x0dbfa420);
    check!("st4 {{v0.d, v1.d, v2.d, v3.d}}[0], [x1], x4", 0x0da4a420);
    check!("st4 {{v0.d, v1.d, v2.d, v3.d}}[1], [x1]", 0x4d20a420);
    check!("st4 {{v0.d, v1.d, v2.d, v3.d}}[1], [x1], #32", 0x4dbfa420);
    check!("st4 {{v0.d, v1.d, v2.d, v3.d}}[1], [x1], x4", 0x4da4a420);
    check!("ld1r {{v0.8b}}, [x1]", 0x0d40c020);
    check!("ld1r {{v0.8b}}, [x1], #1", 0x0ddfc020);
    check!("ld1r {{v0.8b}}, [x1], x4", 0x0dc4c020);
    check!("ld1r {{v0.16b}}, [x1]", 0x4d40c020);
    check!("ld1r {{v0.16b}}, [x1], #1", 0x4ddfc020);
    check!("ld1r {{v0.16b}}, [x1], x4", 0x4dc4c020);
    check!("ld1r {{v0.4h}}, [x1]", 0x0d40c420);
    check!("ld1r {{v0.4h}}, [x1], #2", 0x0ddfc420);
    check!("ld1r {{v0.4h}}, [x1], x4", 0x0dc4c420);
    check!("ld1r {{v0.8h}}, [x1]", 0x4d40c420);
    check!("ld1r {{v0.8h}}, [x1], #2", 0x4ddfc420);
    check!("ld1r {{v0.8h}}, [x1], x4", 0x4dc4c420);
    check!("ld1r {{v0.2s}}, [x1]", 0x0d40c820);
    check!("ld1r {{v0.2s}}, [x1], #4", 0x0ddfc820);
    check!("ld1r {{v0.2s}}, [x1], x4", 0x0dc4c820);
    check!("ld1r {{v0.4s}}, [x1]", 0x4d40c820);
    check!("ld1r {{v0.4s}}, [x1], #4", 0x4ddfc820);
    check!("ld1r {{v0.4s}}, [x1], x4", 0x4dc4c820);
    check!("ld1r {{v0.1d}}, [x1]", 0x0d40cc20);
    check!("ld1r {{v0.1d}}, [x1], #8", 0x0ddfcc20);
    check!("ld1r {{v0.1d}}, [x1], x4", 0x0dc4cc20);
    check!("ld1r {{v0.2d}}, [x1]", 0x4d40cc20);
    check!("ld1r {{v0.2d}}, [x1], #8", 0x4ddfcc20);
    check!("ld1r {{v0.2d}}, [x1], x4", 0x4dc4cc20);
    check!("ld2r {{v0.8b, v1.8b}}, [x1]", 0x0d60c020);
    check!("ld2r {{v0.8b, v1.8b}}, [x1], #2", 0x0dffc020);
    check!("ld2r {{v0.8b, v1.8b}}, [x1], x4", 0x0de4c020);
    check!("ld2r {{v0.16b, v1.16b}}, [x1]", 0x4d60c020);
    check!("ld2r {{v0.16b, v1.16b}}, [x1], #2", 0x4dffc020);
    check!("ld2r {{v0.16b, v1.16b}}, [x1], x4", 0x4de4c020);
    check!("ld2r {{v0.4h, v1.4h}}, [x1]", 0x0d60c420);
    check!("ld2r {{v0.4h, v1.4h}}, [x1], #4", 0x0dffc420);
    check!("ld2r {{v0.4h, v1.4h}}, [x1], x4", 0x0de4c420);
    check!("ld2r {{v0.8h, v1.8h}}, [x1]", 0x4d60c420);
    check!("ld2r {{v0.8h, v1.8h}}, [x1], #4", 0x4dffc420);
    check!("ld2r {{v0.8h, v1.8h}}, [x1], x4", 0x4de4c420);
    check!("ld2r {{v0.2s, v1.2s}}, [x1]", 0x0d60c820);
    check!("ld2r {{v0.2s, v1.2s}}, [x1], #8", 0x0dffc820);
    check!("ld2r {{v0.2s, v1.2s}}, [x1], x4", 0x0de4c820);
    check!("ld2r {{v0.4s, v1.4s}}, [x1]", 0x4d60c820);
    check!("ld2r {{v0.4s, v1.4s}}, [x1], #8", 0x4dffc820);
    check!("ld2r {{v0.4s, v1.4s}}, [x1], x4", 0x4de4c820);
    check!("ld2r {{v0.1d, v1.1d}}, [x1]", 0x0d60cc20);
    check!("ld2r {{v0.1d, v1.1d}}, [x1], #16", 0x0dffcc20);
    check!("ld2r {{v0.1d, v1.1d}}, [x1], x4", 0x0de4cc20);
    check!("ld2r {{v0.2d, v1.2d}}, [x1]", 0x4d60cc20);
    check!("ld2r {{v0.2d, v1.2d}}, [x1], #16", 0x4dffcc20);
    check!("ld2r {{v0.2d, v1.2d}}, [x1], x4", 0x4de4cc20);
    check!("ld3r {{v0.8b, v1.8b, v2.8b}}, [x1]", 0x0d40e020);
    check!("ld3r {{v0.8b, v1.8b, v2.8b}}, [x1], #3", 0x0ddfe020);
    check!("ld3r {{v0.8b, v1.8b, v2.8b}}, [x1], x4", 0x0dc4e020);
    check!("ld3r {{v0.16b, v1.16b, v2.16b}}, [x1]", 0x4d40e020);
    check!("ld3r {{v0.16b, v1.16b, v2.16b}}, [x1], #3", 0x4ddfe020);
    check!("ld3r {{v0.16b, v1.16b, v2.16b}}, [x1], x4", 0x4dc4e020);
    check!("ld3r {{v0.4h, v1.4h, v2.4h}}, [x1]", 0x0d40e420);
    check!("ld3r {{v0.4h, v1.4h, v2.4h}}, [x1], #6", 0x0ddfe420);
    check!("ld3r {{v0.4h, v1.4h, v2.4h}}, [x1], x4", 0x0dc4e420);
    check!("ld3r {{v0.8h, v1.8h, v2.8h}}, [x1]", 0x4d40e420);
    check!("ld3r {{v0.8h, v1.8h, v2.8h}}, [x1], #6", 0x4ddfe420);
    check!("ld3r {{v0.8h, v1.8h, v2.8h}}, [x1], x4", 0x4dc4e420);
    check!("ld3r {{v0.2s, v1.2s, v2.2s}}, [x1]", 0x0d40e820);
    check!("ld3r {{v0.2s, v1.2s, v2.2s}}, [x1], #12", 0x0ddfe820);
    check!("ld3r {{v0.2s, v1.2s, v2.2s}}, [x1], x4", 0x0dc4e820);
    check!("ld3r {{v0.4s, v1.4s, v2.4s}}, [x1]", 0x4d40e820);
    check!("ld3r {{v0.4s, v1.4s, v2.4s}}, [x1], #12", 0x4ddfe820);
    check!("ld3r {{v0.4s, v1.4s, v2.4s}}, [x1], x4", 0x4dc4e820);
    check!("ld3r {{v0.1d, v1.1d, v2.1d}}, [x1]", 0x0d40ec20);
    check!("ld3r {{v0.1d, v1.1d, v2.1d}}, [x1], #24", 0x0ddfec20);
    check!("ld3r {{v0.1d, v1.1d, v2.1d}}, [x1], x4", 0x0dc4ec20);
    check!("ld3r {{v0.2d, v1.2d, v2.2d}}, [x1]", 0x4d40ec20);
    check!("ld3r {{v0.2d, v1.2d, v2.2d}}, [x1], #24", 0x4ddfec20);
    check!("ld3r {{v0.2d, v1.2d, v2.2d}}, [x1], x4", 0x4dc4ec20);
    check!("ld4r {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1]", 0x0d60e020);
    check!("ld4r {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], #4", 0x0dffe020);
    check!("ld4r {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], x4", 0x0de4e020);
    check!("ld4r {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1]", 0x4d60e020);
    check!(
        "ld4r {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], #4",
        0x4dffe020
    );
    check!(
        "ld4r {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], x4",
        0x4de4e020
    );
    check!("ld4r {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1]", 0x0d60e420);
    check!("ld4r {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], #8", 0x0dffe420);
    check!("ld4r {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], x4", 0x0de4e420);
    check!("ld4r {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1]", 0x4d60e420);
    check!("ld4r {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], #8", 0x4dffe420);
    check!("ld4r {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], x4", 0x4de4e420);
    check!("ld4r {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1]", 0x0d60e820);
    check!("ld4r {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], #16", 0x0dffe820);
    check!("ld4r {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], x4", 0x0de4e820);
    check!("ld4r {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1]", 0x4d60e820);
    check!("ld4r {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], #16", 0x4dffe820);
    check!("ld4r {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], x4", 0x4de4e820);
    check!("ld4r {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1]", 0x0d60ec20);
    check!("ld4r {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1], #32", 0x0dffec20);
    check!("ld4r {{v0.1d, v1.1d, v2.1d, v3.1d}}, [x1], x4", 0x0de4ec20);
    check!("ld4r {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1]", 0x4d60ec20);
    check!("ld4r {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], #32", 0x4dffec20);
    check!("ld4r {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], x4", 0x4de4ec20);
    check!("ld2 {{v0.8b, v1.8b}}, [x1]", 0x0c408020);
    check!("ld2 {{v0.8b, v1.8b}}, [x1], #16", 0x0cdf8020);
    check!("ld2 {{v0.8b, v1.8b}}, [x1], x4", 0x0cc48020);
    check!("ld2 {{v0.16b, v1.16b}}, [x1]", 0x4c408020);
    check!("ld2 {{v0.16b, v1.16b}}, [x1], #32", 0x4cdf8020);
    check!("ld2 {{v0.16b, v1.16b}}, [x1], x4", 0x4cc48020);
    check!("ld2 {{v0.4h, v1.4h}}, [x1]", 0x0c408420);
    check!("ld2 {{v0.4h, v1.4h}}, [x1], #16", 0x0cdf8420);
    check!("ld2 {{v0.4h, v1.4h}}, [x1], x4", 0x0cc48420);
    check!("ld2 {{v0.8h, v1.8h}}, [x1]", 0x4c408420);
    check!("ld2 {{v0.8h, v1.8h}}, [x1], #32", 0x4cdf8420);
    check!("ld2 {{v0.8h, v1.8h}}, [x1], x4", 0x4cc48420);
    check!("ld2 {{v0.2s, v1.2s}}, [x1]", 0x0c408820);
    check!("ld2 {{v0.2s, v1.2s}}, [x1], #16", 0x0cdf8820);
    check!("ld2 {{v0.2s, v1.2s}}, [x1], x4", 0x0cc48820);
    check!("ld2 {{v0.4s, v1.4s}}, [x1]", 0x4c408820);
    check!("ld2 {{v0.4s, v1.4s}}, [x1], #32", 0x4cdf8820);
    check!("ld2 {{v0.4s, v1.4s}}, [x1], x4", 0x4cc48820);
    check!("ld2 {{v0.2d, v1.2d}}, [x1]", 0x4c408c20);
    check!("ld2 {{v0.2d, v1.2d}}, [x1], #32", 0x4cdf8c20);
    check!("ld2 {{v0.2d, v1.2d}}, [x1], x4", 0x4cc48c20);
    check!("ld3 {{v0.8b, v1.8b, v2.8b}}, [x1]", 0x0c404020);
    check!("ld3 {{v0.8b, v1.8b, v2.8b}}, [x1], #24", 0x0cdf4020);
    check!("ld3 {{v0.8b, v1.8b, v2.8b}}, [x1], x4", 0x0cc44020);
    check!("ld3 {{v0.16b, v1.16b, v2.16b}}, [x1]", 0x4c404020);
    check!("ld3 {{v0.16b, v1.16b, v2.16b}}, [x1], #48", 0x4cdf4020);
    check!("ld3 {{v0.16b, v1.16b, v2.16b}}, [x1], x4", 0x4cc44020);
    check!("ld3 {{v0.4h, v1.4h, v2.4h}}, [x1]", 0x0c404420);
    check!("ld3 {{v0.4h, v1.4h, v2.4h}}, [x1], #24", 0x0cdf4420);
    check!("ld3 {{v0.4h, v1.4h, v2.4h}}, [x1], x4", 0x0cc44420);
    check!("ld3 {{v0.8h, v1.8h, v2.8h}}, [x1]", 0x4c404420);
    check!("ld3 {{v0.8h, v1.8h, v2.8h}}, [x1], #48", 0x4cdf4420);
    check!("ld3 {{v0.8h, v1.8h, v2.8h}}, [x1], x4", 0x4cc44420);
    check!("ld3 {{v0.2s, v1.2s, v2.2s}}, [x1]", 0x0c404820);
    check!("ld3 {{v0.2s, v1.2s, v2.2s}}, [x1], #24", 0x0cdf4820);
    check!("ld3 {{v0.2s, v1.2s, v2.2s}}, [x1], x4", 0x0cc44820);
    check!("ld3 {{v0.4s, v1.4s, v2.4s}}, [x1]", 0x4c404820);
    check!("ld3 {{v0.4s, v1.4s, v2.4s}}, [x1], #48", 0x4cdf4820);
    check!("ld3 {{v0.4s, v1.4s, v2.4s}}, [x1], x4", 0x4cc44820);
    check!("ld3 {{v0.2d, v1.2d, v2.2d}}, [x1]", 0x4c404c20);
    check!("ld3 {{v0.2d, v1.2d, v2.2d}}, [x1], #48", 0x4cdf4c20);
    check!("ld3 {{v0.2d, v1.2d, v2.2d}}, [x1], x4", 0x4cc44c20);
    check!("ld4 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1]", 0x0c400020);
    check!("ld4 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], #32", 0x0cdf0020);
    check!("ld4 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], x4", 0x0cc40020);
    check!("ld4 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1]", 0x4c400020);
    check!(
        "ld4 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], #64",
        0x4cdf0020
    );
    check!(
        "ld4 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], x4",
        0x4cc40020
    );
    check!("ld4 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1]", 0x0c400420);
    check!("ld4 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], #32", 0x0cdf0420);
    check!("ld4 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], x4", 0x0cc40420);
    check!("ld4 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1]", 0x4c400420);
    check!("ld4 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], #64", 0x4cdf0420);
    check!("ld4 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], x4", 0x4cc40420);
    check!("ld4 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1]", 0x0c400820);
    check!("ld4 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], #32", 0x0cdf0820);
    check!("ld4 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], x4", 0x0cc40820);
    check!("ld4 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1]", 0x4c400820);
    check!("ld4 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], #64", 0x4cdf0820);
    check!("ld4 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], x4", 0x4cc40820);
    check!("ld4 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1]", 0x4c400c20);
    check!("ld4 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], #64", 0x4cdf0c20);
    check!("ld4 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], x4", 0x4cc40c20);
    check!("st2 {{v0.8b, v1.8b}}, [x1]", 0x0c008020);
    check!("st2 {{v0.8b, v1.8b}}, [x1], #16", 0x0c9f8020);
    check!("st2 {{v0.8b, v1.8b}}, [x1], x4", 0x0c848020);
    check!("st2 {{v0.16b, v1.16b}}, [x1]", 0x4c008020);
    check!("st2 {{v0.16b, v1.16b}}, [x1], #32", 0x4c9f8020);
    check!("st2 {{v0.16b, v1.16b}}, [x1], x4", 0x4c848020);
    check!("st2 {{v0.4h, v1.4h}}, [x1]", 0x0c008420);
    check!("st2 {{v0.4h, v1.4h}}, [x1], #16", 0x0c9f8420);
    check!("st2 {{v0.4h, v1.4h}}, [x1], x4", 0x0c848420);
    check!("st2 {{v0.8h, v1.8h}}, [x1]", 0x4c008420);
    check!("st2 {{v0.8h, v1.8h}}, [x1], #32", 0x4c9f8420);
    check!("st2 {{v0.8h, v1.8h}}, [x1], x4", 0x4c848420);
    check!("st2 {{v0.2s, v1.2s}}, [x1]", 0x0c008820);
    check!("st2 {{v0.2s, v1.2s}}, [x1], #16", 0x0c9f8820);
    check!("st2 {{v0.2s, v1.2s}}, [x1], x4", 0x0c848820);
    check!("st2 {{v0.4s, v1.4s}}, [x1]", 0x4c008820);
    check!("st2 {{v0.4s, v1.4s}}, [x1], #32", 0x4c9f8820);
    check!("st2 {{v0.4s, v1.4s}}, [x1], x4", 0x4c848820);
    check!("st2 {{v0.2d, v1.2d}}, [x1]", 0x4c008c20);
    check!("st2 {{v0.2d, v1.2d}}, [x1], #32", 0x4c9f8c20);
    check!("st2 {{v0.2d, v1.2d}}, [x1], x4", 0x4c848c20);
    check!("st3 {{v0.8b, v1.8b, v2.8b}}, [x1]", 0x0c004020);
    check!("st3 {{v0.8b, v1.8b, v2.8b}}, [x1], #24", 0x0c9f4020);
    check!("st3 {{v0.8b, v1.8b, v2.8b}}, [x1], x4", 0x0c844020);
    check!("st3 {{v0.16b, v1.16b, v2.16b}}, [x1]", 0x4c004020);
    check!("st3 {{v0.16b, v1.16b, v2.16b}}, [x1], #48", 0x4c9f4020);
    check!("st3 {{v0.16b, v1.16b, v2.16b}}, [x1], x4", 0x4c844020);
    check!("st3 {{v0.4h, v1.4h, v2.4h}}, [x1]", 0x0c004420);
    check!("st3 {{v0.4h, v1.4h, v2.4h}}, [x1], #24", 0x0c9f4420);
    check!("st3 {{v0.4h, v1.4h, v2.4h}}, [x1], x4", 0x0c844420);
    check!("st3 {{v0.8h, v1.8h, v2.8h}}, [x1]", 0x4c004420);
    check!("st3 {{v0.8h, v1.8h, v2.8h}}, [x1], #48", 0x4c9f4420);
    check!("st3 {{v0.8h, v1.8h, v2.8h}}, [x1], x4", 0x4c844420);
    check!("st3 {{v0.2s, v1.2s, v2.2s}}, [x1]", 0x0c004820);
    check!("st3 {{v0.2s, v1.2s, v2.2s}}, [x1], #24", 0x0c9f4820);
    check!("st3 {{v0.2s, v1.2s, v2.2s}}, [x1], x4", 0x0c844820);
    check!("st3 {{v0.4s, v1.4s, v2.4s}}, [x1]", 0x4c004820);
    check!("st3 {{v0.4s, v1.4s, v2.4s}}, [x1], #48", 0x4c9f4820);
    check!("st3 {{v0.4s, v1.4s, v2.4s}}, [x1], x4", 0x4c844820);
    check!("st3 {{v0.2d, v1.2d, v2.2d}}, [x1]", 0x4c004c20);
    check!("st3 {{v0.2d, v1.2d, v2.2d}}, [x1], #48", 0x4c9f4c20);
    check!("st3 {{v0.2d, v1.2d, v2.2d}}, [x1], x4", 0x4c844c20);
    check!("st4 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1]", 0x0c000020);
    check!("st4 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], #32", 0x0c9f0020);
    check!("st4 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [x1], x4", 0x0c840020);
    check!("st4 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1]", 0x4c000020);
    check!(
        "st4 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], #64",
        0x4c9f0020
    );
    check!(
        "st4 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [x1], x4",
        0x4c840020
    );
    check!("st4 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1]", 0x0c000420);
    check!("st4 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], #32", 0x0c9f0420);
    check!("st4 {{v0.4h, v1.4h, v2.4h, v3.4h}}, [x1], x4", 0x0c840420);
    check!("st4 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1]", 0x4c000420);
    check!("st4 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], #64", 0x4c9f0420);
    check!("st4 {{v0.8h, v1.8h, v2.8h, v3.8h}}, [x1], x4", 0x4c840420);
    check!("st4 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1]", 0x0c000820);
    check!("st4 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], #32", 0x0c9f0820);
    check!("st4 {{v0.2s, v1.2s, v2.2s, v3.2s}}, [x1], x4", 0x0c840820);
    check!("st4 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1]", 0x4c000820);
    check!("st4 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], #64", 0x4c9f0820);
    check!("st4 {{v0.4s, v1.4s, v2.4s, v3.4s}}, [x1], x4", 0x4c840820);
    check!("st4 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1]", 0x4c000c20);
    check!("st4 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], #64", 0x4c9f0c20);
    check!("st4 {{v0.2d, v1.2d, v2.2d, v3.2d}}, [x1], x4", 0x4c840c20);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn immediate_shifts_match_native_signed_rounding_and_insert_forms() {
    #[inline(never)]
    fn check<const WORD: u32>(reference: fn(u128, u128) -> u128) {
        for source in [0, u128::MAX, 0x8000_0001_7fff_ffff_80ff_7f01_8001_7fff] {
            for previous in [0, u128::MAX, 0x1234_5678_9abc_def0_fedc_ba98_7654_3210] {
                let expected = reference(source, previous);
                let mut cpu = cpu_for(WORD);
                cpu.v[0] = source;
                cpu.v[2] = previous;
                cpu.nzcv = 0xa;
                cpu.sysregs.fpsr = 0x0800_009f;
                cpu.step().unwrap();
                assert_eq!(
                    cpu.v[2], expected,
                    "word={WORD:#x} input={source:#x} prior={previous:#x}"
                );
                assert_eq!(cpu.nzcv, 0xa);
                assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
            }
        }
    }
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            check::<$word>(|source, previous| {
                let mut result = 0u128;
                // SAFETY: fixed compile-time instruction and valid 16-byte buffers.
                unsafe {
                    std::arch::asm!(
                        "ldr q0, [{source}]", "ldr q2, [{previous}]", $instruction,
                        "str q2, [{result}]",
                        source = in(reg) &source, previous = in(reg) &previous,
                        result = in(reg) &mut result,
                        out("v0") _, out("v2") _, options(nostack, preserves_flags),
                    );
                }
                result
            });
        };
    }
    check!("shl v2.8b, v0.8b, #0", 0x0f085402);
    check!("shl v2.8b, v0.8b, #4", 0x0f0c5402);
    check!("shl v2.8b, v0.8b, #7", 0x0f0f5402);
    check!("shl v2.16b, v0.16b, #0", 0x4f085402);
    check!("shl v2.16b, v0.16b, #4", 0x4f0c5402);
    check!("shl v2.16b, v0.16b, #7", 0x4f0f5402);
    check!("shl v2.4h, v0.4h, #0", 0x0f105402);
    check!("shl v2.4h, v0.4h, #8", 0x0f185402);
    check!("shl v2.4h, v0.4h, #15", 0x0f1f5402);
    check!("shl v2.8h, v0.8h, #0", 0x4f105402);
    check!("shl v2.8h, v0.8h, #8", 0x4f185402);
    check!("shl v2.8h, v0.8h, #15", 0x4f1f5402);
    check!("shl v2.2s, v0.2s, #0", 0x0f205402);
    check!("shl v2.2s, v0.2s, #16", 0x0f305402);
    check!("shl v2.2s, v0.2s, #31", 0x0f3f5402);
    check!("shl v2.4s, v0.4s, #0", 0x4f205402);
    check!("shl v2.4s, v0.4s, #16", 0x4f305402);
    check!("shl v2.4s, v0.4s, #31", 0x4f3f5402);
    check!("shl v2.2d, v0.2d, #0", 0x4f405402);
    check!("shl v2.2d, v0.2d, #32", 0x4f605402);
    check!("shl v2.2d, v0.2d, #63", 0x4f7f5402);
    check!("shl d2, d0, #0", 0x5f405402);
    check!("shl d2, d0, #32", 0x5f605402);
    check!("shl d2, d0, #63", 0x5f7f5402);
    check!("sli v2.8b, v0.8b, #0", 0x2f085402);
    check!("sli v2.8b, v0.8b, #4", 0x2f0c5402);
    check!("sli v2.8b, v0.8b, #7", 0x2f0f5402);
    check!("sli v2.16b, v0.16b, #0", 0x6f085402);
    check!("sli v2.16b, v0.16b, #4", 0x6f0c5402);
    check!("sli v2.16b, v0.16b, #7", 0x6f0f5402);
    check!("sli v2.4h, v0.4h, #0", 0x2f105402);
    check!("sli v2.4h, v0.4h, #8", 0x2f185402);
    check!("sli v2.4h, v0.4h, #15", 0x2f1f5402);
    check!("sli v2.8h, v0.8h, #0", 0x6f105402);
    check!("sli v2.8h, v0.8h, #8", 0x6f185402);
    check!("sli v2.8h, v0.8h, #15", 0x6f1f5402);
    check!("sli v2.2s, v0.2s, #0", 0x2f205402);
    check!("sli v2.2s, v0.2s, #16", 0x2f305402);
    check!("sli v2.2s, v0.2s, #31", 0x2f3f5402);
    check!("sli v2.4s, v0.4s, #0", 0x6f205402);
    check!("sli v2.4s, v0.4s, #16", 0x6f305402);
    check!("sli v2.4s, v0.4s, #31", 0x6f3f5402);
    check!("sli v2.2d, v0.2d, #0", 0x6f405402);
    check!("sli v2.2d, v0.2d, #32", 0x6f605402);
    check!("sli v2.2d, v0.2d, #63", 0x6f7f5402);
    check!("sli d2, d0, #0", 0x7f405402);
    check!("sli d2, d0, #32", 0x7f605402);
    check!("sli d2, d0, #63", 0x7f7f5402);
    check!("sri v2.8b, v0.8b, #1", 0x2f0f4402);
    check!("sri v2.8b, v0.8b, #4", 0x2f0c4402);
    check!("sri v2.8b, v0.8b, #8", 0x2f084402);
    check!("sri v2.16b, v0.16b, #1", 0x6f0f4402);
    check!("sri v2.16b, v0.16b, #4", 0x6f0c4402);
    check!("sri v2.16b, v0.16b, #8", 0x6f084402);
    check!("sri v2.4h, v0.4h, #1", 0x2f1f4402);
    check!("sri v2.4h, v0.4h, #8", 0x2f184402);
    check!("sri v2.4h, v0.4h, #16", 0x2f104402);
    check!("sri v2.8h, v0.8h, #1", 0x6f1f4402);
    check!("sri v2.8h, v0.8h, #8", 0x6f184402);
    check!("sri v2.8h, v0.8h, #16", 0x6f104402);
    check!("sri v2.2s, v0.2s, #1", 0x2f3f4402);
    check!("sri v2.2s, v0.2s, #16", 0x2f304402);
    check!("sri v2.2s, v0.2s, #32", 0x2f204402);
    check!("sri v2.4s, v0.4s, #1", 0x6f3f4402);
    check!("sri v2.4s, v0.4s, #16", 0x6f304402);
    check!("sri v2.4s, v0.4s, #32", 0x6f204402);
    check!("sri v2.2d, v0.2d, #1", 0x6f7f4402);
    check!("sri v2.2d, v0.2d, #32", 0x6f604402);
    check!("sri v2.2d, v0.2d, #64", 0x6f404402);
    check!("sri d2, d0, #1", 0x7f7f4402);
    check!("sri d2, d0, #32", 0x7f604402);
    check!("sri d2, d0, #64", 0x7f404402);
    check!("sshr v2.8b, v0.8b, #1", 0x0f0f0402);
    check!("sshr v2.8b, v0.8b, #4", 0x0f0c0402);
    check!("sshr v2.8b, v0.8b, #8", 0x0f080402);
    check!("sshr v2.16b, v0.16b, #1", 0x4f0f0402);
    check!("sshr v2.16b, v0.16b, #4", 0x4f0c0402);
    check!("sshr v2.16b, v0.16b, #8", 0x4f080402);
    check!("sshr v2.4h, v0.4h, #1", 0x0f1f0402);
    check!("sshr v2.4h, v0.4h, #8", 0x0f180402);
    check!("sshr v2.4h, v0.4h, #16", 0x0f100402);
    check!("sshr v2.8h, v0.8h, #1", 0x4f1f0402);
    check!("sshr v2.8h, v0.8h, #8", 0x4f180402);
    check!("sshr v2.8h, v0.8h, #16", 0x4f100402);
    check!("sshr v2.2s, v0.2s, #1", 0x0f3f0402);
    check!("sshr v2.2s, v0.2s, #16", 0x0f300402);
    check!("sshr v2.2s, v0.2s, #32", 0x0f200402);
    check!("sshr v2.4s, v0.4s, #1", 0x4f3f0402);
    check!("sshr v2.4s, v0.4s, #16", 0x4f300402);
    check!("sshr v2.4s, v0.4s, #32", 0x4f200402);
    check!("sshr v2.2d, v0.2d, #1", 0x4f7f0402);
    check!("sshr v2.2d, v0.2d, #32", 0x4f600402);
    check!("sshr v2.2d, v0.2d, #64", 0x4f400402);
    check!("sshr d2, d0, #1", 0x5f7f0402);
    check!("sshr d2, d0, #32", 0x5f600402);
    check!("sshr d2, d0, #64", 0x5f400402);
    check!("ushr v2.8b, v0.8b, #1", 0x2f0f0402);
    check!("ushr v2.8b, v0.8b, #4", 0x2f0c0402);
    check!("ushr v2.8b, v0.8b, #8", 0x2f080402);
    check!("ushr v2.16b, v0.16b, #1", 0x6f0f0402);
    check!("ushr v2.16b, v0.16b, #4", 0x6f0c0402);
    check!("ushr v2.16b, v0.16b, #8", 0x6f080402);
    check!("ushr v2.4h, v0.4h, #1", 0x2f1f0402);
    check!("ushr v2.4h, v0.4h, #8", 0x2f180402);
    check!("ushr v2.4h, v0.4h, #16", 0x2f100402);
    check!("ushr v2.8h, v0.8h, #1", 0x6f1f0402);
    check!("ushr v2.8h, v0.8h, #8", 0x6f180402);
    check!("ushr v2.8h, v0.8h, #16", 0x6f100402);
    check!("ushr v2.2s, v0.2s, #1", 0x2f3f0402);
    check!("ushr v2.2s, v0.2s, #16", 0x2f300402);
    check!("ushr v2.2s, v0.2s, #32", 0x2f200402);
    check!("ushr v2.4s, v0.4s, #1", 0x6f3f0402);
    check!("ushr v2.4s, v0.4s, #16", 0x6f300402);
    check!("ushr v2.4s, v0.4s, #32", 0x6f200402);
    check!("ushr v2.2d, v0.2d, #1", 0x6f7f0402);
    check!("ushr v2.2d, v0.2d, #32", 0x6f600402);
    check!("ushr v2.2d, v0.2d, #64", 0x6f400402);
    check!("ushr d2, d0, #1", 0x7f7f0402);
    check!("ushr d2, d0, #32", 0x7f600402);
    check!("ushr d2, d0, #64", 0x7f400402);
    check!("ssra v2.8b, v0.8b, #1", 0x0f0f1402);
    check!("ssra v2.8b, v0.8b, #4", 0x0f0c1402);
    check!("ssra v2.8b, v0.8b, #8", 0x0f081402);
    check!("ssra v2.16b, v0.16b, #1", 0x4f0f1402);
    check!("ssra v2.16b, v0.16b, #4", 0x4f0c1402);
    check!("ssra v2.16b, v0.16b, #8", 0x4f081402);
    check!("ssra v2.4h, v0.4h, #1", 0x0f1f1402);
    check!("ssra v2.4h, v0.4h, #8", 0x0f181402);
    check!("ssra v2.4h, v0.4h, #16", 0x0f101402);
    check!("ssra v2.8h, v0.8h, #1", 0x4f1f1402);
    check!("ssra v2.8h, v0.8h, #8", 0x4f181402);
    check!("ssra v2.8h, v0.8h, #16", 0x4f101402);
    check!("ssra v2.2s, v0.2s, #1", 0x0f3f1402);
    check!("ssra v2.2s, v0.2s, #16", 0x0f301402);
    check!("ssra v2.2s, v0.2s, #32", 0x0f201402);
    check!("ssra v2.4s, v0.4s, #1", 0x4f3f1402);
    check!("ssra v2.4s, v0.4s, #16", 0x4f301402);
    check!("ssra v2.4s, v0.4s, #32", 0x4f201402);
    check!("ssra v2.2d, v0.2d, #1", 0x4f7f1402);
    check!("ssra v2.2d, v0.2d, #32", 0x4f601402);
    check!("ssra v2.2d, v0.2d, #64", 0x4f401402);
    check!("ssra d2, d0, #1", 0x5f7f1402);
    check!("ssra d2, d0, #32", 0x5f601402);
    check!("ssra d2, d0, #64", 0x5f401402);
    check!("usra v2.8b, v0.8b, #1", 0x2f0f1402);
    check!("usra v2.8b, v0.8b, #4", 0x2f0c1402);
    check!("usra v2.8b, v0.8b, #8", 0x2f081402);
    check!("usra v2.16b, v0.16b, #1", 0x6f0f1402);
    check!("usra v2.16b, v0.16b, #4", 0x6f0c1402);
    check!("usra v2.16b, v0.16b, #8", 0x6f081402);
    check!("usra v2.4h, v0.4h, #1", 0x2f1f1402);
    check!("usra v2.4h, v0.4h, #8", 0x2f181402);
    check!("usra v2.4h, v0.4h, #16", 0x2f101402);
    check!("usra v2.8h, v0.8h, #1", 0x6f1f1402);
    check!("usra v2.8h, v0.8h, #8", 0x6f181402);
    check!("usra v2.8h, v0.8h, #16", 0x6f101402);
    check!("usra v2.2s, v0.2s, #1", 0x2f3f1402);
    check!("usra v2.2s, v0.2s, #16", 0x2f301402);
    check!("usra v2.2s, v0.2s, #32", 0x2f201402);
    check!("usra v2.4s, v0.4s, #1", 0x6f3f1402);
    check!("usra v2.4s, v0.4s, #16", 0x6f301402);
    check!("usra v2.4s, v0.4s, #32", 0x6f201402);
    check!("usra v2.2d, v0.2d, #1", 0x6f7f1402);
    check!("usra v2.2d, v0.2d, #32", 0x6f601402);
    check!("usra v2.2d, v0.2d, #64", 0x6f401402);
    check!("usra d2, d0, #1", 0x7f7f1402);
    check!("usra d2, d0, #32", 0x7f601402);
    check!("usra d2, d0, #64", 0x7f401402);
    check!("srshr v2.8b, v0.8b, #1", 0x0f0f2402);
    check!("srshr v2.8b, v0.8b, #4", 0x0f0c2402);
    check!("srshr v2.8b, v0.8b, #8", 0x0f082402);
    check!("srshr v2.16b, v0.16b, #1", 0x4f0f2402);
    check!("srshr v2.16b, v0.16b, #4", 0x4f0c2402);
    check!("srshr v2.16b, v0.16b, #8", 0x4f082402);
    check!("srshr v2.4h, v0.4h, #1", 0x0f1f2402);
    check!("srshr v2.4h, v0.4h, #8", 0x0f182402);
    check!("srshr v2.4h, v0.4h, #16", 0x0f102402);
    check!("srshr v2.8h, v0.8h, #1", 0x4f1f2402);
    check!("srshr v2.8h, v0.8h, #8", 0x4f182402);
    check!("srshr v2.8h, v0.8h, #16", 0x4f102402);
    check!("srshr v2.2s, v0.2s, #1", 0x0f3f2402);
    check!("srshr v2.2s, v0.2s, #16", 0x0f302402);
    check!("srshr v2.2s, v0.2s, #32", 0x0f202402);
    check!("srshr v2.4s, v0.4s, #1", 0x4f3f2402);
    check!("srshr v2.4s, v0.4s, #16", 0x4f302402);
    check!("srshr v2.4s, v0.4s, #32", 0x4f202402);
    check!("srshr v2.2d, v0.2d, #1", 0x4f7f2402);
    check!("srshr v2.2d, v0.2d, #32", 0x4f602402);
    check!("srshr v2.2d, v0.2d, #64", 0x4f402402);
    check!("srshr d2, d0, #1", 0x5f7f2402);
    check!("srshr d2, d0, #32", 0x5f602402);
    check!("srshr d2, d0, #64", 0x5f402402);
    check!("urshr v2.8b, v0.8b, #1", 0x2f0f2402);
    check!("urshr v2.8b, v0.8b, #4", 0x2f0c2402);
    check!("urshr v2.8b, v0.8b, #8", 0x2f082402);
    check!("urshr v2.16b, v0.16b, #1", 0x6f0f2402);
    check!("urshr v2.16b, v0.16b, #4", 0x6f0c2402);
    check!("urshr v2.16b, v0.16b, #8", 0x6f082402);
    check!("urshr v2.4h, v0.4h, #1", 0x2f1f2402);
    check!("urshr v2.4h, v0.4h, #8", 0x2f182402);
    check!("urshr v2.4h, v0.4h, #16", 0x2f102402);
    check!("urshr v2.8h, v0.8h, #1", 0x6f1f2402);
    check!("urshr v2.8h, v0.8h, #8", 0x6f182402);
    check!("urshr v2.8h, v0.8h, #16", 0x6f102402);
    check!("urshr v2.2s, v0.2s, #1", 0x2f3f2402);
    check!("urshr v2.2s, v0.2s, #16", 0x2f302402);
    check!("urshr v2.2s, v0.2s, #32", 0x2f202402);
    check!("urshr v2.4s, v0.4s, #1", 0x6f3f2402);
    check!("urshr v2.4s, v0.4s, #16", 0x6f302402);
    check!("urshr v2.4s, v0.4s, #32", 0x6f202402);
    check!("urshr v2.2d, v0.2d, #1", 0x6f7f2402);
    check!("urshr v2.2d, v0.2d, #32", 0x6f602402);
    check!("urshr v2.2d, v0.2d, #64", 0x6f402402);
    check!("urshr d2, d0, #1", 0x7f7f2402);
    check!("urshr d2, d0, #32", 0x7f602402);
    check!("urshr d2, d0, #64", 0x7f402402);
    check!("srsra v2.8b, v0.8b, #1", 0x0f0f3402);
    check!("srsra v2.8b, v0.8b, #4", 0x0f0c3402);
    check!("srsra v2.8b, v0.8b, #8", 0x0f083402);
    check!("srsra v2.16b, v0.16b, #1", 0x4f0f3402);
    check!("srsra v2.16b, v0.16b, #4", 0x4f0c3402);
    check!("srsra v2.16b, v0.16b, #8", 0x4f083402);
    check!("srsra v2.4h, v0.4h, #1", 0x0f1f3402);
    check!("srsra v2.4h, v0.4h, #8", 0x0f183402);
    check!("srsra v2.4h, v0.4h, #16", 0x0f103402);
    check!("srsra v2.8h, v0.8h, #1", 0x4f1f3402);
    check!("srsra v2.8h, v0.8h, #8", 0x4f183402);
    check!("srsra v2.8h, v0.8h, #16", 0x4f103402);
    check!("srsra v2.2s, v0.2s, #1", 0x0f3f3402);
    check!("srsra v2.2s, v0.2s, #16", 0x0f303402);
    check!("srsra v2.2s, v0.2s, #32", 0x0f203402);
    check!("srsra v2.4s, v0.4s, #1", 0x4f3f3402);
    check!("srsra v2.4s, v0.4s, #16", 0x4f303402);
    check!("srsra v2.4s, v0.4s, #32", 0x4f203402);
    check!("srsra v2.2d, v0.2d, #1", 0x4f7f3402);
    check!("srsra v2.2d, v0.2d, #32", 0x4f603402);
    check!("srsra v2.2d, v0.2d, #64", 0x4f403402);
    check!("srsra d2, d0, #1", 0x5f7f3402);
    check!("srsra d2, d0, #32", 0x5f603402);
    check!("srsra d2, d0, #64", 0x5f403402);
    check!("ursra v2.8b, v0.8b, #1", 0x2f0f3402);
    check!("ursra v2.8b, v0.8b, #4", 0x2f0c3402);
    check!("ursra v2.8b, v0.8b, #8", 0x2f083402);
    check!("ursra v2.16b, v0.16b, #1", 0x6f0f3402);
    check!("ursra v2.16b, v0.16b, #4", 0x6f0c3402);
    check!("ursra v2.16b, v0.16b, #8", 0x6f083402);
    check!("ursra v2.4h, v0.4h, #1", 0x2f1f3402);
    check!("ursra v2.4h, v0.4h, #8", 0x2f183402);
    check!("ursra v2.4h, v0.4h, #16", 0x2f103402);
    check!("ursra v2.8h, v0.8h, #1", 0x6f1f3402);
    check!("ursra v2.8h, v0.8h, #8", 0x6f183402);
    check!("ursra v2.8h, v0.8h, #16", 0x6f103402);
    check!("ursra v2.2s, v0.2s, #1", 0x2f3f3402);
    check!("ursra v2.2s, v0.2s, #16", 0x2f303402);
    check!("ursra v2.2s, v0.2s, #32", 0x2f203402);
    check!("ursra v2.4s, v0.4s, #1", 0x6f3f3402);
    check!("ursra v2.4s, v0.4s, #16", 0x6f303402);
    check!("ursra v2.4s, v0.4s, #32", 0x6f203402);
    check!("ursra v2.2d, v0.2d, #1", 0x6f7f3402);
    check!("ursra v2.2d, v0.2d, #32", 0x6f603402);
    check!("ursra v2.2d, v0.2d, #64", 0x6f403402);
    check!("ursra d2, d0, #1", 0x7f7f3402);
    check!("ursra d2, d0, #32", 0x7f603402);
    check!("ursra d2, d0, #64", 0x7f403402);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn scalar_dup_matches_native_all_lanes_and_widths() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for source in [0u128, u128::MAX, 0x1234_5678_9abc_def0_fedc_ba98_7654_3210] {
                let mut expected = 0u128;
                // SAFETY: fixed instruction and initialized 16-byte buffers.
                unsafe {
                    std::arch::asm!("ldr q0, [{source}]", $instruction, "str q2, [{result}]",
                        source = in(reg) &source, result = in(reg) &mut expected,
                        out("v0") _, out("v2") _, options(nostack, preserves_flags));
                }
                let mut cpu = cpu_for($word);
                cpu.v[0] = source;
                cpu.v[2] = u128::MAX;
                cpu.step().unwrap();
                assert_eq!(cpu.v[2], expected, "{} source={source:#x}", $instruction);
            }
        };
    }
    check!("dup b2, v0.b[0]", 0x5e010402);
    check!("dup b2, v0.b[1]", 0x5e030402);
    check!("dup b2, v0.b[2]", 0x5e050402);
    check!("dup b2, v0.b[3]", 0x5e070402);
    check!("dup b2, v0.b[4]", 0x5e090402);
    check!("dup b2, v0.b[5]", 0x5e0b0402);
    check!("dup b2, v0.b[6]", 0x5e0d0402);
    check!("dup b2, v0.b[7]", 0x5e0f0402);
    check!("dup b2, v0.b[8]", 0x5e110402);
    check!("dup b2, v0.b[9]", 0x5e130402);
    check!("dup b2, v0.b[10]", 0x5e150402);
    check!("dup b2, v0.b[11]", 0x5e170402);
    check!("dup b2, v0.b[12]", 0x5e190402);
    check!("dup b2, v0.b[13]", 0x5e1b0402);
    check!("dup b2, v0.b[14]", 0x5e1d0402);
    check!("dup b2, v0.b[15]", 0x5e1f0402);
    check!("dup h2, v0.h[0]", 0x5e020402);
    check!("dup h2, v0.h[1]", 0x5e060402);
    check!("dup h2, v0.h[2]", 0x5e0a0402);
    check!("dup h2, v0.h[3]", 0x5e0e0402);
    check!("dup h2, v0.h[4]", 0x5e120402);
    check!("dup h2, v0.h[5]", 0x5e160402);
    check!("dup h2, v0.h[6]", 0x5e1a0402);
    check!("dup h2, v0.h[7]", 0x5e1e0402);
    check!("dup s2, v0.s[0]", 0x5e040402);
    check!("dup s2, v0.s[1]", 0x5e0c0402);
    check!("dup s2, v0.s[2]", 0x5e140402);
    check!("dup s2, v0.s[3]", 0x5e1c0402);
    check!("dup d2, v0.d[0]", 0x5e080402);
    check!("dup d2, v0.d[1]", 0x5e180402);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn integer_reductions_match_native_all_arrangements() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for source in [0u128, u128::MAX, 0x8000_0000_7fff_ffff_8001_7ffe_00ff_0100,
                0xffff_ffff_ffff_ffff_0807_0605_0403_0201] {
                let mut native = 0u128;
                // SAFETY: only fixed assembly, initialized local vector buffers.
                unsafe { std::arch::asm!(
                    "ldr q0, [{source}]", $instruction, "str q2, [{output}]",
                    source = in(reg) &source, output = in(reg) &mut native,
                    out("v0") _, out("v2") _, options(nostack, preserves_flags),
                ); }
                let mut cpu = cpu_for($word);
                cpu.v[0] = source;
                cpu.v[2] = u128::MAX;
                cpu.nzcv = 0xa;
                cpu.sysregs.fpsr = 0x0800_009f;
                cpu.step().unwrap();
                assert_eq!(cpu.v[2], native, "{} source={source:#x}", $instruction);
                assert_eq!(cpu.v[0], source);
                assert_eq!(cpu.nzcv, 0xa);
                assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
            }
        };
    }
    check!("addp d2, v0.2d", 0x5ef1b802);
    check!("addv b2, v0.8b", 0x0e31b802);
    check!("addv b2, v0.16b", 0x4e31b802);
    check!("addv h2, v0.4h", 0x0e71b802);
    check!("addv h2, v0.8h", 0x4e71b802);
    check!("addv s2, v0.4s", 0x4eb1b802);
    check!("saddlv h2, v0.8b", 0x0e303802);
    check!("saddlv h2, v0.16b", 0x4e303802);
    check!("saddlv s2, v0.4h", 0x0e703802);
    check!("saddlv s2, v0.8h", 0x4e703802);
    check!("saddlv d2, v0.4s", 0x4eb03802);
    check!("uaddlv h2, v0.8b", 0x2e303802);
    check!("uaddlv h2, v0.16b", 0x6e303802);
    check!("uaddlv s2, v0.4h", 0x2e703802);
    check!("uaddlv s2, v0.8h", 0x6e703802);
    check!("uaddlv d2, v0.4s", 0x6eb03802);
    check!("smaxv b2, v0.8b", 0x0e30a802);
    check!("smaxv b2, v0.16b", 0x4e30a802);
    check!("smaxv h2, v0.4h", 0x0e70a802);
    check!("smaxv h2, v0.8h", 0x4e70a802);
    check!("smaxv s2, v0.4s", 0x4eb0a802);
    check!("umaxv b2, v0.8b", 0x2e30a802);
    check!("umaxv b2, v0.16b", 0x6e30a802);
    check!("umaxv h2, v0.4h", 0x2e70a802);
    check!("umaxv h2, v0.8h", 0x6e70a802);
    check!("umaxv s2, v0.4s", 0x6eb0a802);
    check!("sminv b2, v0.8b", 0x0e31a802);
    check!("sminv b2, v0.16b", 0x4e31a802);
    check!("sminv h2, v0.4h", 0x0e71a802);
    check!("sminv h2, v0.8h", 0x4e71a802);
    check!("sminv s2, v0.4s", 0x4eb1a802);
    check!("uminv b2, v0.8b", 0x2e31a802);
    check!("uminv b2, v0.16b", 0x6e31a802);
    check!("uminv h2, v0.4h", 0x2e71a802);
    check!("uminv h2, v0.8h", 0x6e71a802);
    check!("uminv s2, v0.4s", 0x6eb1a802);
}

#[test]
fn integer_reductions_alias_and_reserved_arrangements() {
    let mut cpu = cpu_for(0x2e30_a800); // measured UMAXV B0,V0.8B
    cpu.v[0] = 0xffff_ffff_ffff_ffff_0807_0605_0403_0201;
    cpu.step().unwrap();
    assert_eq!(cpu.v[0], 8);
    for base in [
        0x0e31_b800,
        0x0e30_3800,
        0x2e30_3800,
        0x0e30_a800,
        0x2e30_a800,
        0x0e31_a800,
        0x2e31_a800,
    ] {
        for invalid in [
            base | (2 << 22),
            base | (3 << 22),
            base | (3 << 22) | (1 << 30),
        ] {
            let mut cpu = cpu_for(invalid);
            cpu.v[0] = u128::MAX;
            assert!(cpu.step().is_err(), "{invalid:#x}");
            assert_eq!(cpu.v[0], u128::MAX);
            assert_eq!(cpu.pc, 0);
        }
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn table_lookup_matches_native_all_lengths_and_aliases() {
    let tables: [u128; 4] = std::array::from_fn(|table| {
        u128::from_le_bytes(std::array::from_fn(|byte| (table * 16 + byte) as u8))
    });
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for indices in [0, u128::MAX, 0x0f0e_0d0c_0b0a_0908_0706_0504_0302_0100,
                0xff40_3f30_2f20_1f10_0f00_ff40_3f30_2f20] {
                for alias in [false, true] {
                    let prior = if alias { tables[0] } else { 0xa5a5_a5a5_a5a5_a5a5_a5a5_a5a5_a5a5_a5a5 };
                    let mut native = 0u128;
                    // SAFETY: fixed assembly and valid local buffers; no guest code execution.
                    unsafe { std::arch::asm!(
                        "ldp q0, q1, [{tables}]", "ldp q2, q3, [{tables}, #32]",
                        "ldr q4, [{indices}]", "ldr q5, [{prior}]", $instruction,
                        "str q5, [{result}]",
                        tables = in(reg) &tables, indices = in(reg) &indices,
                        prior = in(reg) &prior, result = in(reg) &mut native,
                        out("v0") _, out("v1") _, out("v2") _, out("v3") _,
                        out("v4") _, out("v5") _, options(nostack, preserves_flags),
                    ); }
                    let word = if alias {
                        ($word & !(31 | (31 << 5) | (31 << 16))) | 31 | (31 << 5) | (28 << 16)
                    } else { $word };
                    let mut cpu = cpu_for(word);
                    for i in 0..4 { cpu.v[(if alias { 31 } else { 0 } + i) & 31] = tables[i]; }
                    cpu.v[if alias { 28 } else { 4 }] = indices;
                    let rd = if alias { 31 } else { 5 };
                    cpu.v[rd] = prior;
                    cpu.nzcv = 0xa;
                    cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[rd], native, "{} alias={alias} indices={indices:#x}", $instruction);
                    assert_eq!(cpu.nzcv, 0xa);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
                }
            }
        };
    }
    check!("tbl v5.8b, {{v0.16b}}, v4.8b", 0x0e040005);
    check!("tbl v5.16b, {{v0.16b}}, v4.16b", 0x4e040005);
    check!("tbl v5.8b, {{v0.16b, v1.16b}}, v4.8b", 0x0e042005);
    check!("tbl v5.16b, {{v0.16b, v1.16b}}, v4.16b", 0x4e042005);
    check!("tbl v5.8b, {{v0.16b, v1.16b, v2.16b}}, v4.8b", 0x0e044005);
    check!("tbl v5.16b, {{v0.16b, v1.16b, v2.16b}}, v4.16b", 0x4e044005);
    check!(
        "tbl v5.8b, {{v0.16b, v1.16b, v2.16b, v3.16b}}, v4.8b",
        0x0e046005
    );
    check!(
        "tbl v5.16b, {{v0.16b, v1.16b, v2.16b, v3.16b}}, v4.16b",
        0x4e046005
    );
    check!("tbx v5.8b, {{v0.16b}}, v4.8b", 0x0e041005);
    check!("tbx v5.16b, {{v0.16b}}, v4.16b", 0x4e041005);
    check!("tbx v5.8b, {{v0.16b, v1.16b}}, v4.8b", 0x0e043005);
    check!("tbx v5.16b, {{v0.16b, v1.16b}}, v4.16b", 0x4e043005);
    check!("tbx v5.8b, {{v0.16b, v1.16b, v2.16b}}, v4.8b", 0x0e045005);
    check!("tbx v5.16b, {{v0.16b, v1.16b, v2.16b}}, v4.16b", 0x4e045005);
    check!(
        "tbx v5.8b, {{v0.16b, v1.16b, v2.16b, v3.16b}}, v4.8b",
        0x0e047005
    );
    check!(
        "tbx v5.16b, {{v0.16b, v1.16b, v2.16b, v3.16b}}, v4.16b",
        0x4e047005
    );
}

#[test]
fn table_lookup_boundaries_and_overlapping_indices() {
    let tables = [u128::MAX; 4];
    assert!(simd_table_lookup(tables, 0, 0, 0, true, false).is_none());
    assert!(simd_table_lookup(tables, 5, 0, 0, true, false).is_none());
    let mut cpu = cpu_for(0x0e05_1005); // TBX V5.8B,{V0.16B},V5.8B
    cpu.v[0] = 0x0f0e_0d0c_0b0a_0908_0706_0504_0302_0100;
    cpu.v[5] = 0xffff_ffff_ffff_ffff_ff10_0f00_0111_02fe;
    cpu.step().unwrap();
    assert_eq!(cpu.v[5], 0xff10_0f00_0111_02fe);
    let mut cpu = cpu_for(0x4e1c_03ff); // measured in-place TBL V31.16B,{V31.16B},V28.16B
    cpu.v[31] = u128::MAX;
    cpu.v[28] = 0x100f_100f_100f_100f_100f_100f_100f_100f;
    cpu.step().unwrap();
    assert_eq!(cpu.v[31], 0x00ff_00ff_00ff_00ff_00ff_00ff_00ff_00ff);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn saturating_arithmetic_matches_native_all_widths_and_flags() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for (left, right) in [(0u128, u128::MAX), (u128::MAX, u128::MAX), (5, 3),
                (0x7f7f_7f7f_7f7f_7f7f_7f7f_7f7f_7f7f_7f7f, 0x7f7f_7f7f_7f7f_7f7f_7f7f_7f7f_7f7f_7f7f),
                (0x8080_8080_8080_8080_8080_8080_8080_8080, 0x8080_8080_8080_8080_8080_8080_8080_8080),
                (0x8000_0000_0000_0000_7fff_ffff_ffff_ffff, 0x0000_0000_0000_0001_0000_0000_0000_0001)] {
                for initial in [0x9fu64, 0x0800_009f] {
                    let mut native = 0u128;
                    let status: u64;
                    // SAFETY: fixed arithmetic, valid local buffers, host FPSR restored.
                    unsafe { std::arch::asm!(
                        "mrs x9, fpsr", "msr fpsr, {initial}",
                        "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction,
                        "str q2, [{output}]", "mrs {status}, fpsr", "msr fpsr, x9",
                        initial = in(reg) initial, left = in(reg) &left, right = in(reg) &right,
                        output = in(reg) &mut native, status = lateout(reg) status,
                        out("x9") _, out("v0") _, out("v1") _, out("v2") _,
                        options(nostack, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = left;
                    cpu.v[1] = right;
                    cpu.v[2] = u128::MAX;
                    cpu.nzcv = 0xa;
                    cpu.sysregs.fpsr = initial;
                    cpu.step().unwrap();
                    assert_eq!((cpu.v[2], cpu.sysregs.fpsr), (native, status),
                        "{} left={left:#x} right={right:#x} initial={initial:#x}", $instruction);
                    assert_eq!(cpu.nzcv, 0xa);
                }
            }
        };
    }
    check!("sqadd v2.8b, v0.8b, v1.8b", 0x0e210c02);
    check!("sqadd v2.16b, v0.16b, v1.16b", 0x4e210c02);
    check!("sqadd v2.4h, v0.4h, v1.4h", 0x0e610c02);
    check!("sqadd v2.8h, v0.8h, v1.8h", 0x4e610c02);
    check!("sqadd v2.2s, v0.2s, v1.2s", 0x0ea10c02);
    check!("sqadd v2.4s, v0.4s, v1.4s", 0x4ea10c02);
    check!("sqadd v2.2d, v0.2d, v1.2d", 0x4ee10c02);
    check!("sqadd b2, b0, b1", 0x5e210c02);
    check!("sqadd h2, h0, h1", 0x5e610c02);
    check!("sqadd s2, s0, s1", 0x5ea10c02);
    check!("sqadd d2, d0, d1", 0x5ee10c02);
    check!("uqadd v2.8b, v0.8b, v1.8b", 0x2e210c02);
    check!("uqadd v2.16b, v0.16b, v1.16b", 0x6e210c02);
    check!("uqadd v2.4h, v0.4h, v1.4h", 0x2e610c02);
    check!("uqadd v2.8h, v0.8h, v1.8h", 0x6e610c02);
    check!("uqadd v2.2s, v0.2s, v1.2s", 0x2ea10c02);
    check!("uqadd v2.4s, v0.4s, v1.4s", 0x6ea10c02);
    check!("uqadd v2.2d, v0.2d, v1.2d", 0x6ee10c02);
    check!("uqadd b2, b0, b1", 0x7e210c02);
    check!("uqadd h2, h0, h1", 0x7e610c02);
    check!("uqadd s2, s0, s1", 0x7ea10c02);
    check!("uqadd d2, d0, d1", 0x7ee10c02);
    check!("sqsub v2.8b, v0.8b, v1.8b", 0x0e212c02);
    check!("sqsub v2.16b, v0.16b, v1.16b", 0x4e212c02);
    check!("sqsub v2.4h, v0.4h, v1.4h", 0x0e612c02);
    check!("sqsub v2.8h, v0.8h, v1.8h", 0x4e612c02);
    check!("sqsub v2.2s, v0.2s, v1.2s", 0x0ea12c02);
    check!("sqsub v2.4s, v0.4s, v1.4s", 0x4ea12c02);
    check!("sqsub v2.2d, v0.2d, v1.2d", 0x4ee12c02);
    check!("sqsub b2, b0, b1", 0x5e212c02);
    check!("sqsub h2, h0, h1", 0x5e612c02);
    check!("sqsub s2, s0, s1", 0x5ea12c02);
    check!("sqsub d2, d0, d1", 0x5ee12c02);
    check!("uqsub v2.8b, v0.8b, v1.8b", 0x2e212c02);
    check!("uqsub v2.16b, v0.16b, v1.16b", 0x6e212c02);
    check!("uqsub v2.4h, v0.4h, v1.4h", 0x2e612c02);
    check!("uqsub v2.8h, v0.8h, v1.8h", 0x6e612c02);
    check!("uqsub v2.2s, v0.2s, v1.2s", 0x2ea12c02);
    check!("uqsub v2.4s, v0.4s, v1.4s", 0x6ea12c02);
    check!("uqsub v2.2d, v0.2d, v1.2d", 0x6ee12c02);
    check!("uqsub b2, b0, b1", 0x7e212c02);
    check!("uqsub h2, h0, h1", 0x7e612c02);
    check!("uqsub s2, s0, s1", 0x7ea12c02);
    check!("uqsub d2, d0, d1", 0x7ee12c02);
}

#[test]
fn saturating_arithmetic_aliases_and_reserved_width() {
    let mut cpu = cpu_for(0x6efd_2f9c); // measured UQSUB V28.2D,V28.2D,V29.2D
    cpu.v[28] = 8u128 << 64;
    cpu.v[29] = (3u128 << 64) | 5;
    cpu.sysregs.fpsr = 0x9f;
    cpu.step().unwrap();
    assert_eq!(cpu.v[28], 5u128 << 64);
    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
    cpu.pc = 0;
    cpu.v[28] = (8u128 << 64) | 6;
    cpu.step().unwrap();
    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f); // QC is sticky.
    for word in [0x0ee1_0c02, 0x2ee1_0c02, 0x0ee1_2c02, 0x2ee1_2c02] {
        let mut cpu = cpu_for(word);
        cpu.v[2] = u128::MAX;
        assert!(cpu.step().is_err());
        assert_eq!(cpu.v[2], u128::MAX);
        assert_eq!(cpu.sysregs.fpsr, 0);
        assert_eq!(cpu.pc, 0);
    }
    assert!(saturating_lane(0, 0, 0, false, false).is_none());
    let mut cpu = cpu_for(0x5ef1_bbff); // ADDP D31,V31.2D
    cpu.v[31] = (1u128 << 64) | u128::from(u64::MAX);
    cpu.step().unwrap();
    assert_eq!(cpu.v[31], 0);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn integer_minmax_matches_native_all_arrangements() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for (left, right) in [(0u128, u128::MAX), (u128::MAX, 0),
                (0x8080_8080_8080_8080_7f7f_7f7f_7f7f_7f7f, 0x7f7f_7f7f_7f7f_7f7f_8080_8080_8080_8080),
                (0x8000_8000_8000_8000_7fff_7fff_7fff_7fff, 0x7fff_7fff_7fff_7fff_8000_8000_8000_8000),
                (0x8000_0000_8000_0000_7fff_ffff_7fff_ffff, 0x7fff_ffff_7fff_ffff_8000_0000_8000_0000),
                (0x1234, 0x1234)] {
                let mut native = 0u128;
                // SAFETY: fixed min/max instructions and initialized local buffers.
                unsafe { std::arch::asm!(
                    "ldr q0, [{left}]", "ldr q1, [{right}]", $instruction, "str q2, [{output}]",
                    left = in(reg) &left, right = in(reg) &right, output = in(reg) &mut native,
                    out("v0") _, out("v1") _, out("v2") _, options(nostack, preserves_flags),
                ); }
                let mut cpu = cpu_for($word);
                cpu.v[0] = left; cpu.v[1] = right; cpu.v[2] = u128::MAX;
                cpu.nzcv = 0xa; cpu.sysregs.fpsr = 0x0800_009f;
                cpu.step().unwrap();
                assert_eq!(cpu.v[2], native, "{} left={left:#x} right={right:#x}", $instruction);
                assert_eq!(cpu.nzcv, 0xa); assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
            }
        };
    }
    check!("smax v2.8b, v0.8b, v1.8b", 0x0e216402);
    check!("smax v2.16b, v0.16b, v1.16b", 0x4e216402);
    check!("smax v2.4h, v0.4h, v1.4h", 0x0e616402);
    check!("smax v2.8h, v0.8h, v1.8h", 0x4e616402);
    check!("smax v2.2s, v0.2s, v1.2s", 0x0ea16402);
    check!("smax v2.4s, v0.4s, v1.4s", 0x4ea16402);
    check!("umax v2.8b, v0.8b, v1.8b", 0x2e216402);
    check!("umax v2.16b, v0.16b, v1.16b", 0x6e216402);
    check!("umax v2.4h, v0.4h, v1.4h", 0x2e616402);
    check!("umax v2.8h, v0.8h, v1.8h", 0x6e616402);
    check!("umax v2.2s, v0.2s, v1.2s", 0x2ea16402);
    check!("umax v2.4s, v0.4s, v1.4s", 0x6ea16402);
    check!("smin v2.8b, v0.8b, v1.8b", 0x0e216c02);
    check!("smin v2.16b, v0.16b, v1.16b", 0x4e216c02);
    check!("smin v2.4h, v0.4h, v1.4h", 0x0e616c02);
    check!("smin v2.8h, v0.8h, v1.8h", 0x4e616c02);
    check!("smin v2.2s, v0.2s, v1.2s", 0x0ea16c02);
    check!("smin v2.4s, v0.4s, v1.4s", 0x4ea16c02);
    check!("umin v2.8b, v0.8b, v1.8b", 0x2e216c02);
    check!("umin v2.16b, v0.16b, v1.16b", 0x6e216c02);
    check!("umin v2.4h, v0.4h, v1.4h", 0x2e616c02);
    check!("umin v2.8h, v0.8h, v1.8h", 0x6e616c02);
    check!("umin v2.2s, v0.2s, v1.2s", 0x2ea16c02);
    check!("umin v2.4s, v0.4s, v1.4s", 0x6ea16c02);
}

#[test]
fn integer_minmax_aliases_and_reserved_widths() {
    let mut cpu = cpu_for(0x6ebf_67bd); // Measured UMAX V29.4S,V29.4S,V31.4S.
    cpu.v[29] = 0xffff_ffff_0000_0000_8000_0000_7fff_ffff;
    cpu.v[31] = 0x0000_0000_ffff_ffff_7fff_ffff_8000_0000;
    cpu.step().unwrap();
    assert_eq!(cpu.v[29], 0xffff_ffff_ffff_ffff_8000_0000_8000_0000);
    for word in [0x0ee1_6402, 0x4ee1_6402, 0x2ee1_6c02, 0x6ee1_6c02] {
        let mut cpu = cpu_for(word);
        cpu.v = [u128::MAX; 32];
        assert!(cpu.step().is_err());
        assert_eq!(cpu.v, [u128::MAX; 32]);
        assert_eq!(cpu.pc, 0);
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn leading_count_matches_native_all_arrangements() {
    macro_rules! check {
        ($instruction:literal, $word:expr) => {
            for source in [0u128, u128::MAX,
                0x8080_8080_8080_8080_7f7f_7f7f_7f7f_7f7f,
                0x8000_8000_8000_8000_7fff_7fff_7fff_7fff,
                0x8000_0000_8000_0000_7fff_ffff_7fff_ffff,
                0x0102_0408_1020_4080_ffff_0000_0000_0001] {
                let mut native = 0u128;
                // SAFETY: fixed instructions and initialized local buffers.
                unsafe { std::arch::asm!(
                    "ldr q0, [{source}]", $instruction, "str q2, [{output}]",
                    source = in(reg) &source, output = in(reg) &mut native,
                    out("v0") _, out("v2") _, options(nostack, preserves_flags),
                ); }
                let mut cpu = cpu_for($word);
                cpu.v[0] = source; cpu.v[2] = u128::MAX;
                cpu.nzcv = 0xa; cpu.sysregs.fpsr = 0x0800_009f;
                cpu.step().unwrap();
                assert_eq!(cpu.v[2], native, "{} source={source:#x}", $instruction);
                assert_eq!(cpu.nzcv, 0xa); assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
            }
        };
    }
    check!("clz v2.8b, v0.8b", 0x2e204802);
    check!("clz v2.16b, v0.16b", 0x6e204802);
    check!("clz v2.4h, v0.4h", 0x2e604802);
    check!("clz v2.8h, v0.8h", 0x6e604802);
    check!("clz v2.2s, v0.2s", 0x2ea04802);
    check!("clz v2.4s, v0.4s", 0x6ea04802);
    check!("cls v2.8b, v0.8b", 0x0e204802);
    check!("cls v2.16b, v0.16b", 0x4e204802);
    check!("cls v2.4h, v0.4h", 0x0e604802);
    check!("cls v2.8h, v0.8h", 0x4e604802);
    check!("cls v2.2s, v0.2s", 0x0ea04802);
    check!("cls v2.4s, v0.4s", 0x4ea04802);
}

#[test]
fn leading_count_boundaries_aliases_and_reserved_widths() {
    let mut cpu = cpu_for(0x6ea0_4b7b); // Measured CLZ V27.4S,V27.4S.
    cpu.v[27] = 0x8000_0000_0000_0001_0000_0000_7fff_ffff;
    cpu.step().unwrap();
    assert_eq!(cpu.v[27], 0x0000_0000_0000_001f_0000_0020_0000_0001);
    for bits in [8, 16, 32] {
        for value in [0, 1, low_mask(bits), 1 << (bits - 1), (1 << (bits - 1)) - 1] {
            for signed in [false, true] {
                let target = signed && value & (1 << (bits - 1)) != 0;
                let expected = (0..bits - u32::from(signed))
                    .rev()
                    .take_while(|bit| (value & (1 << bit) != 0) == target)
                    .count() as u32;
                assert_eq!(leading_count_lane(value, bits, signed), expected);
            }
        }
    }
    for word in [0x0ee0_4802, 0x4ee0_4802, 0x2ee0_4802, 0x6ee0_4802] {
        let mut cpu = cpu_for(word);
        cpu.v = [u128::MAX; 32];
        assert!(cpu.step().is_err());
        assert_eq!(cpu.v, [u128::MAX; 32]);
        assert_eq!(cpu.pc, 0);
    }
}

#[test]
fn logical_immediate_stack_destination_and_zero_source() {
    let mut cpu = cpu_for(0x9279_e13f); // Measured AND SP,X9,#-128.
    cpu.sp = 0x1000;
    cpu.x[9] = 0xd40;
    cpu.nzcv = 0xa;
    cpu.step().unwrap();
    assert_eq!(cpu.sp, 0xd00);
    assert_eq!(cpu.nzcv, 0xa);
    for wide in [false, true] {
        for operation in 0..4 {
            // Immediate #0xff, Rn=0, Rd=31. ANDS alone discards Rd=31.
            let word =
                0x1200_1c1f | (u32::from(wide) << 31) | (u32::from(wide) << 22) | (operation << 29);
            let mut cpu = cpu_for(word);
            cpu.sp = 0x1234_5678_0000_1000;
            cpu.x[0] = 0xfedc_ba98_7654_3210;
            cpu.nzcv = 0xa;
            cpu.step().unwrap();
            let expected = match operation {
                0 | 3 => cpu.x[0] & 255,
                1 => cpu.x[0] | 255,
                _ => cpu.x[0] ^ 255,
            } & if wide { u64::MAX } else { u64::from(u32::MAX) };
            assert_eq!(
                cpu.sp,
                if operation == 3 {
                    0x1234_5678_0000_1000
                } else {
                    expected
                }
            );
            assert_eq!(cpu.nzcv, if operation == 3 { 0 } else { 0xa });
            // Source 31 remains ZR even when destination is SP.
            let mut cpu = cpu_for(word | (31 << 5));
            cpu.sp = 0x1000;
            cpu.step().unwrap();
            assert_eq!(
                cpu.sp,
                match operation {
                    0 => 0,
                    1 | 2 => 255,
                    _ => 0x1000,
                }
            );
            if operation == 3 {
                assert_eq!(cpu.nzcv, 4);
            }
        }
    }
}

#[test]
fn logical_immediate_stack_alignment_preserves_caller_frame() {
    let mut cpu = cpu_for(0xa9bc_7bfd);
    let words: [u32; 7] = [
        0xa9bc_7bfd, // STP X29,X30,[SP,#-64]!
        0x9100_03fd, // MOV X29,SP
        0xd10b_03e9, // SUB X9,SP,#0x2c0
        0x9279_e13f, // AND SP,X9,#-128
        0xf900_03ff, // STR XZR,[SP], must not overwrite the saved frame.
        0x9100_03bf, // MOV SP,X29
        0xa8c4_7bfd, // LDP X29,X30,[SP],#64
    ];
    for (i, word) in words.into_iter().enumerate() {
        cpu.memory
            .write((i * 4) as u64, &word.to_le_bytes())
            .unwrap();
    }
    cpu.sp = 4096;
    cpu.x[29] = 0x1234;
    cpu.x[30] = 0x5678;
    cpu.run_slice(7).unwrap();
    assert_eq!((cpu.sp, cpu.x[29], cpu.x[30]), (4096, 0x1234, 0x5678));
}

#[test]
fn conditional_float_compare_skips_nan_and_preserves_operands() {
    for double in [false, true] {
        for signaling in [false, true] {
            for immediate in 0..16 {
                let word =
                    0x1e21_0400 | ((double as u32) << 22) | ((signaling as u32) << 4) | immediate;
                let mut cpu = cpu_for(word);
                cpu.v[0] = u128::MAX;
                cpu.v[1] = u128::MAX;
                cpu.nzcv = 0; // EQ false: NaNs must not raise IOC.
                cpu.sysregs.fpsr = 2;
                cpu.step().unwrap();
                assert_eq!(cpu.nzcv, immediate as u8);
                assert_eq!(cpu.sysregs.fpsr, 2);
                assert_eq!((cpu.v[0], cpu.v[1]), (u128::MAX, u128::MAX));
                cpu.pc = 0;
                cpu.nzcv = 4;
                cpu.step().unwrap();
                assert_eq!(cpu.nzcv, 3); // Unordered.
                assert_eq!(cpu.sysregs.fpsr, if signaling { 3 } else { 2 });
            }
        }
    }
    for word in [0x1ea1_0400, 0x1ee1_0400] {
        assert!(cpu_for(word).step().is_err());
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn conditional_float_compare_matches_native_conditions_and_exceptions() {
    macro_rules! check {
        ($instruction:literal, $word:expr, $double:expr) => {
            for initial in 0..16u64 {
                for sample in [0u64, 1, 0x3ff0_0000_3f80_0000, 0x7ff8_0000_7fc0_0000, 0x7ff0_0000_7f80_0001] {
                    let flags: u64;
                    let status: u64;
                    // SAFETY: fixed instructions; host FP state and NZCV restored.
                    unsafe { std::arch::asm!(
                        "mrs x8, fpcr", "mrs x9, fpsr", "mrs x11, nzcv",
                        "msr fpcr, xzr", "msr fpsr, {sticky}", "msr nzcv, {initial}",
                        "fmov d0, {sample}", "fmov d1, xzr", $instruction,
                        "mrs {flags}, nzcv", "mrs {status}, fpsr",
                        "msr fpcr, x8", "msr fpsr, x9", "msr nzcv, x11",
                        sticky = in(reg) 2u64, initial = in(reg) (initial << 28),
                        sample = in(reg) sample, flags = out(reg) flags, status = out(reg) status,
                        out("x8") _, out("x9") _, out("x11") _, out("v0") _, out("v1") _,
                        options(nostack, nomem, preserves_flags),
                    ); }
                    let mut cpu = cpu_for($word);
                    cpu.v[0] = sample as u128;
                    cpu.nzcv = initial as u8;
                    cpu.sysregs.fpsr = 2;
                    cpu.step().unwrap();
                    assert_eq!((cpu.nzcv, cpu.sysregs.fpsr), ((flags >> 28) as u8, status),
                        "{} initial={initial} sample={sample:#x}", $instruction);
                }
            }
        };
    }
    check!("fccmp s0, s1, #10, eq", 0x1e21040a, false);
    check!("fccmpe s0, s1, #10, eq", 0x1e21041a, false);
    check!("fccmp d0, d1, #10, eq", 0x1e61040a, true);
    check!("fccmpe d0, d1, #10, eq", 0x1e61041a, true);
    check!("fccmp s0, s1, #10, ne", 0x1e21140a, false);
    check!("fccmpe s0, s1, #10, ne", 0x1e21141a, false);
    check!("fccmp d0, d1, #10, ne", 0x1e61140a, true);
    check!("fccmpe d0, d1, #10, ne", 0x1e61141a, true);
    check!("fccmp s0, s1, #10, cs", 0x1e21240a, false);
    check!("fccmpe s0, s1, #10, cs", 0x1e21241a, false);
    check!("fccmp d0, d1, #10, cs", 0x1e61240a, true);
    check!("fccmpe d0, d1, #10, cs", 0x1e61241a, true);
    check!("fccmp s0, s1, #10, cc", 0x1e21340a, false);
    check!("fccmpe s0, s1, #10, cc", 0x1e21341a, false);
    check!("fccmp d0, d1, #10, cc", 0x1e61340a, true);
    check!("fccmpe d0, d1, #10, cc", 0x1e61341a, true);
    check!("fccmp s0, s1, #10, mi", 0x1e21440a, false);
    check!("fccmpe s0, s1, #10, mi", 0x1e21441a, false);
    check!("fccmp d0, d1, #10, mi", 0x1e61440a, true);
    check!("fccmpe d0, d1, #10, mi", 0x1e61441a, true);
    check!("fccmp s0, s1, #10, pl", 0x1e21540a, false);
    check!("fccmpe s0, s1, #10, pl", 0x1e21541a, false);
    check!("fccmp d0, d1, #10, pl", 0x1e61540a, true);
    check!("fccmpe d0, d1, #10, pl", 0x1e61541a, true);
    check!("fccmp s0, s1, #10, vs", 0x1e21640a, false);
    check!("fccmpe s0, s1, #10, vs", 0x1e21641a, false);
    check!("fccmp d0, d1, #10, vs", 0x1e61640a, true);
    check!("fccmpe d0, d1, #10, vs", 0x1e61641a, true);
    check!("fccmp s0, s1, #10, vc", 0x1e21740a, false);
    check!("fccmpe s0, s1, #10, vc", 0x1e21741a, false);
    check!("fccmp d0, d1, #10, vc", 0x1e61740a, true);
    check!("fccmpe d0, d1, #10, vc", 0x1e61741a, true);
    check!("fccmp s0, s1, #10, hi", 0x1e21840a, false);
    check!("fccmpe s0, s1, #10, hi", 0x1e21841a, false);
    check!("fccmp d0, d1, #10, hi", 0x1e61840a, true);
    check!("fccmpe d0, d1, #10, hi", 0x1e61841a, true);
    check!("fccmp s0, s1, #10, ls", 0x1e21940a, false);
    check!("fccmpe s0, s1, #10, ls", 0x1e21941a, false);
    check!("fccmp d0, d1, #10, ls", 0x1e61940a, true);
    check!("fccmpe d0, d1, #10, ls", 0x1e61941a, true);
    check!("fccmp s0, s1, #10, ge", 0x1e21a40a, false);
    check!("fccmpe s0, s1, #10, ge", 0x1e21a41a, false);
    check!("fccmp d0, d1, #10, ge", 0x1e61a40a, true);
    check!("fccmpe d0, d1, #10, ge", 0x1e61a41a, true);
    check!("fccmp s0, s1, #10, lt", 0x1e21b40a, false);
    check!("fccmpe s0, s1, #10, lt", 0x1e21b41a, false);
    check!("fccmp d0, d1, #10, lt", 0x1e61b40a, true);
    check!("fccmpe d0, d1, #10, lt", 0x1e61b41a, true);
    check!("fccmp s0, s1, #10, gt", 0x1e21c40a, false);
    check!("fccmpe s0, s1, #10, gt", 0x1e21c41a, false);
    check!("fccmp d0, d1, #10, gt", 0x1e61c40a, true);
    check!("fccmpe d0, d1, #10, gt", 0x1e61c41a, true);
    check!("fccmp s0, s1, #10, le", 0x1e21d40a, false);
    check!("fccmpe s0, s1, #10, le", 0x1e21d41a, false);
    check!("fccmp d0, d1, #10, le", 0x1e61d40a, true);
    check!("fccmpe d0, d1, #10, le", 0x1e61d41a, true);
    check!("fccmp s0, s1, #10, al", 0x1e21e40a, false);
    check!("fccmpe s0, s1, #10, al", 0x1e21e41a, false);
    check!("fccmp d0, d1, #10, al", 0x1e61e40a, true);
    check!("fccmpe d0, d1, #10, al", 0x1e61e41a, true);
    check!("fccmp s0, s1, #10, nv", 0x1e21f40a, false);
    check!("fccmpe s0, s1, #10, nv", 0x1e21f41a, false);
    check!("fccmp d0, d1, #10, nv", 0x1e61f40a, true);
    check!("fccmpe d0, d1, #10, nv", 0x1e61f41a, true);
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn scalar_fused_matches_native_all_forms_controls_results_and_flags() {
    macro_rules! check {
        ($op:literal, $load0:literal, $load1:literal, $load2:literal, $store:literal, $word:expr, $double:expr) => {{
            let mut cpu = cpu_for($word);
            let special: &[u64] = if $double {
                &[0, 1, 0x0010_0000_0000_0000, 0x3ff0_0000_0000_0000,
                  0x3ff0_0000_0000_0001, 0x7fef_ffff_ffff_ffff,
                  0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_1234, 0x7ff8_0000_0000_5678]
            } else {
                &[0, 1, 0x0080_0000, 0x3f80_0000, 0x3f80_0001,
                  0x7f7f_ffff, 0x7f80_0000, 0x7f80_1234, 0x7fc0_5678]
            };
            let sign = 1u64 << if $double { 63 } else { 31 };
            let mut triples = Vec::new();
            for &a in special { for &b in special { for &c in special {
                for signs in 0..8 {
                    triples.push((a | if signs & 1 != 0 { sign } else { 0 },
                                  b | if signs & 2 != 0 { sign } else { 0 },
                                  c | if signs & 4 != 0 { sign } else { 0 }));
                }
            }}}
            let mut random = 0x572d_81aa_bdef_0193u64;
            for _ in 0..512 {
                let mut next = || { random ^= random << 13; random ^= random >> 7;
                    random ^= random << 17; random };
                triples.push((next(), next(), next()));
            }
            for control in 0..16u64 {
                let fpcr = control << 22;
                for &(left, right, addend) in &triples {
                    let native: u64;
                    let status: u64;
                    // SAFETY: only fixed assembler instructions run; host FP
                    // control/status are restored in this block. No guest bytes
                    // execute and no executable memory is allocated.
                    unsafe { std::arch::asm!(
                        "mrs x8, fpcr", "mrs x9, fpsr",
                        "msr fpcr, {control}", "msr fpsr, {initial}",
                        $load0, $load1, $load2, $op, $store,
                        "mrs {status}, fpsr", "msr fpcr, x8", "msr fpsr, x9",
                        "mov {result}, x10",
                        control = in(reg) fpcr, initial = in(reg) 0x0800_0000u64,
                        left = in(reg) left, right = in(reg) right, addend = in(reg) addend,
                        status = out(reg) status, result = out(reg) native,
                        out("x8") _, out("x9") _, out("x10") _,
                        out("v0") _, out("v1") _, out("v2") _, out("v3") _,
                        options(nostack, nomem, preserves_flags),
                    ); }
                    cpu.pc = 0;
                    cpu.v[31] = u128::from(left) | (u128::from(u64::MAX) << 64);
                    cpu.v[30] = u128::from(right);
                    cpu.v[29] = u128::from(addend);
                    cpu.nzcv = 0xa;
                    cpu.sysregs.fpcr = fpcr;
                    cpu.sysregs.fpsr = 0x0800_0000;
                    cpu.step().unwrap();
                    assert_eq!((cpu.v[31], cpu.sysregs.fpsr), (u128::from(native), status),
                        "{} left={left:#018x} right={right:#018x} addend={addend:#018x} fpcr={fpcr:#x}", $op);
                    assert_eq!(cpu.v[30], u128::from(right));
                    assert_eq!(cpu.v[29], u128::from(addend));
                    assert_eq!(cpu.nzcv, 0xa);
                    assert_eq!(cpu.sysregs.fpcr, fpcr);
                    assert_eq!(cpu.pc, 4);
                }
            }
        }};
    }
    check!(
        "fmadd s3, s0, s1, s2",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        "fmov s2, {addend:w}",
        "fmov w10, s3",
        0x1f1e_77ff,
        false
    );
    check!(
        "fmsub s3, s0, s1, s2",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        "fmov s2, {addend:w}",
        "fmov w10, s3",
        0x1f1e_f7ff,
        false
    );
    check!(
        "fnmadd s3, s0, s1, s2",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        "fmov s2, {addend:w}",
        "fmov w10, s3",
        0x1f3e_77ff,
        false
    );
    check!(
        "fnmsub s3, s0, s1, s2",
        "fmov s0, {left:w}",
        "fmov s1, {right:w}",
        "fmov s2, {addend:w}",
        "fmov w10, s3",
        0x1f3e_f7ff,
        false
    );
    check!(
        "fmadd d3, d0, d1, d2",
        "fmov d0, {left}",
        "fmov d1, {right}",
        "fmov d2, {addend}",
        "fmov x10, d3",
        0x1f5e_77ff,
        true
    );
    check!(
        "fmsub d3, d0, d1, d2",
        "fmov d0, {left}",
        "fmov d1, {right}",
        "fmov d2, {addend}",
        "fmov x10, d3",
        0x1f5e_f7ff,
        true
    );
    check!(
        "fnmadd d3, d0, d1, d2",
        "fmov d0, {left}",
        "fmov d1, {right}",
        "fmov d2, {addend}",
        "fmov x10, d3",
        0x1f7e_77ff,
        true
    );
    check!(
        "fnmsub d3, d0, d1, d2",
        "fmov d0, {left}",
        "fmov d1, {right}",
        "fmov d2, {addend}",
        "fmov x10, d3",
        0x1f7e_f7ff,
        true
    );
}

#[test]
fn pairwise_long_guest_alias_and_reserved_encodings() {
    let mut cpu = cpu_for(0x6e202800); // Measured guest: UADDLP V0.8H,V0.16B.
    cpu.v[0] = u128::MAX;
    cpu.nzcv = 0xa;
    cpu.sysregs.fpsr = 0x0800_009f;
    cpu.step().unwrap();
    assert_eq!(cpu.v[0], 0x01fe_01fe_01fe_01fe_01fe_01fe_01fe_01fe);
    assert_eq!(cpu.nzcv, 0xa);
    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f);
    for unsigned in [0, 1 << 29] {
        for wide in [0, 1 << 30] {
            for accumulate in [0, 1 << 14] {
                let mut cpu = cpu_for(0x0ee02800 | unsigned | wide | accumulate);
                cpu.v[0] = u128::MAX;
                assert!(cpu.step().is_err());
                assert_eq!(cpu.v[0], u128::MAX);
                assert_eq!(cpu.pc, 0);
            }
        }
    }
    for invalid in [0, 7, 64, 128] {
        assert!(pairwise_long_lane(0, 0, 0, invalid, false, false).is_none());
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
#[test]
fn pairwise_long_matches_native_all_forms_and_aliases() {
    let mut samples = vec![
        0,
        u128::MAX,
        0x8080_8080_8080_8080_8080_8080_8080_8080,
        0x7f7f_7f7f_7f7f_7f7f_7f7f_7f7f_7f7f_7f7f,
        0x8000_7fff_ffff_0001_8000_7fff_ffff_0001,
        0x8000_0000_7fff_ffff_ffff_ffff_0000_0001,
    ];
    let mut random = 0x834a_941d_ac30_17efu128;
    for _ in 0..64 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        samples.push(random);
    }
    macro_rules! check {
        ($separate:literal, $alias:literal, $word:expr) => {{
            for &source in &samples {
                let prior = source.rotate_left(41) ^ u128::MAX;
                for alias in [false, true] {
                    let mut native = 0u128;
                    // SAFETY: fixed assembler mnemonics only and initialized
                    // 16-byte local data. No guest instruction bytes execute.
                    unsafe {
                        if alias {
                            std::arch::asm!("ldr q0, [{source}]", $alias, "str q0, [{output}]",
                                source=in(reg) &source, output=in(reg) &mut native,
                                out("v0") _, options(nostack, preserves_flags));
                        } else {
                            std::arch::asm!("ldr q0, [{prior}]", "ldr q1, [{source}]", $separate, "str q0, [{output}]",
                                prior=in(reg) &prior, source=in(reg) &source, output=in(reg) &mut native,
                                out("v0") _, out("v1") _, options(nostack, preserves_flags));
                        }
                    }
                    let mut cpu = cpu_for($word | if alias { 0 } else { 31 << 5 });
                    cpu.v[0] = if alias { source } else { prior }; cpu.v[31] = source;
                    cpu.nzcv = 0xa; cpu.sysregs.fpsr = 0x0800_009f;
                    cpu.step().unwrap();
                    assert_eq!(cpu.v[0], native, "{} alias={} source={:#x} prior={:#x}", $separate, alias, source, prior);
                    assert_eq!(cpu.v[31], source); assert_eq!(cpu.nzcv, 0xa);
                    assert_eq!(cpu.sysregs.fpsr, 0x0800_009f); assert_eq!(cpu.pc, 4);
                }
            }
        }};
    }
    check!("saddlp v0.4h, v1.8b", "saddlp v0.4h, v0.8b", 0x0e202800);
    check!("saddlp v0.2s, v1.4h", "saddlp v0.2s, v0.4h", 0x0e602800);
    check!("saddlp v0.1d, v1.2s", "saddlp v0.1d, v0.2s", 0x0ea02800);
    check!("saddlp v0.8h, v1.16b", "saddlp v0.8h, v0.16b", 0x4e202800);
    check!("saddlp v0.4s, v1.8h", "saddlp v0.4s, v0.8h", 0x4e602800);
    check!("saddlp v0.2d, v1.4s", "saddlp v0.2d, v0.4s", 0x4ea02800);
    check!("uaddlp v0.4h, v1.8b", "uaddlp v0.4h, v0.8b", 0x2e202800);
    check!("uaddlp v0.2s, v1.4h", "uaddlp v0.2s, v0.4h", 0x2e602800);
    check!("uaddlp v0.1d, v1.2s", "uaddlp v0.1d, v0.2s", 0x2ea02800);
    check!("uaddlp v0.8h, v1.16b", "uaddlp v0.8h, v0.16b", 0x6e202800);
    check!("uaddlp v0.4s, v1.8h", "uaddlp v0.4s, v0.8h", 0x6e602800);
    check!("uaddlp v0.2d, v1.4s", "uaddlp v0.2d, v0.4s", 0x6ea02800);
    check!("sadalp v0.4h, v1.8b", "sadalp v0.4h, v0.8b", 0x0e206800);
    check!("sadalp v0.2s, v1.4h", "sadalp v0.2s, v0.4h", 0x0e606800);
    check!("sadalp v0.1d, v1.2s", "sadalp v0.1d, v0.2s", 0x0ea06800);
    check!("sadalp v0.8h, v1.16b", "sadalp v0.8h, v0.16b", 0x4e206800);
    check!("sadalp v0.4s, v1.8h", "sadalp v0.4s, v0.8h", 0x4e606800);
    check!("sadalp v0.2d, v1.4s", "sadalp v0.2d, v0.4s", 0x4ea06800);
    check!("uadalp v0.4h, v1.8b", "uadalp v0.4h, v0.8b", 0x2e206800);
    check!("uadalp v0.2s, v1.4h", "uadalp v0.2s, v0.4h", 0x2e606800);
    check!("uadalp v0.1d, v1.2s", "uadalp v0.1d, v0.2s", 0x2ea06800);
    check!("uadalp v0.8h, v1.16b", "uadalp v0.8h, v0.16b", 0x6e206800);
    check!("uadalp v0.4s, v1.8h", "uadalp v0.4s, v0.8h", 0x6e606800);
    check!("uadalp v0.2d, v1.4s", "uadalp v0.2d, v0.4s", 0x6ea06800);
}

mod indexed_float;

mod vector_round;

mod vector_precision;

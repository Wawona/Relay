use wwn_runtime::{Error, Op::*, Program};

#[test]
fn fusion_matches_wrapping_arithmetic_and_all_aliases() {
    let values = [
        0,
        1,
        42,
        u32::MAX as u64,
        i64::MAX as u64,
        1 << 63,
        u64::MAX,
    ];
    for dst in 0..3 {
        for lhs in 0..3 {
            for rhs in 0..3 {
                let ops = [
                    Add { dst, lhs, rhs },
                    Mul {
                        dst: 0,
                        lhs: dst,
                        rhs,
                    },
                    Return { src: 0 },
                ];
                let fused = Program::compile(&ops, 3, true).unwrap();
                let plain = Program::compile(&ops, 3, false).unwrap();
                assert_eq!(fused.fused_pairs(), 1);
                for &a in &values {
                    for &b in &values {
                        let args = [a, b, 7];
                        let mut expected = args;
                        expected[dst as usize] =
                            expected[lhs as usize].wrapping_add(expected[rhs as usize]);
                        expected[0] = expected[dst as usize].wrapping_mul(expected[rhs as usize]);
                        for fuel in 0..5 {
                            let result = fused.run(&args, &mut [], fuel);
                            assert_eq!(result, plain.run(&args, &mut [], fuel));
                            if fuel >= 3 {
                                assert_eq!(result.unwrap().value, expected[0]);
                            } else {
                                assert_eq!(result, Err(Error::OutOfFuel));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn branch_into_second_instruction_prevents_fusion() {
    let ops = [
        Jump { target: 2 },
        Add {
            dst: 0,
            lhs: 0,
            rhs: 1,
        },
        Mul {
            dst: 0,
            lhs: 0,
            rhs: 1,
        },
        Return { src: 0 },
    ];
    let p = Program::compile(&ops, 2, true).unwrap();
    assert_eq!(p.fused_pairs(), 0);
    assert_eq!(p.run(&[3, 4], &mut [], 3).unwrap().value, 12);
}

#[test]
fn loops_are_bounded_and_conditional_fallthrough_works() {
    let ops = [
        JumpIf {
            condition: 0,
            target: 0,
        },
        Return { src: 0 },
    ];
    let p = Program::compile(&ops, 1, true).unwrap();
    assert_eq!(p.run(&[1], &mut [], 1000), Err(Error::OutOfFuel));
    assert_eq!(p.run(&[0], &mut [], 2).unwrap().value, 0);
}

#[test]
fn rejects_invalid_programs_before_execution() {
    assert!(Program::compile(&[], 1, true).is_err());
    assert!(Program::compile(&[Return { src: 0 }], 65537, true).is_err());
    assert!(Program::compile(&[Return { src: 1 }], 1, true).is_err());
    assert!(Program::compile(&[Jump { target: 1 }], 1, true).is_err());
    assert!(Program::compile(&[Const { dst: 0, value: 1 }], 1, true).is_err());
    assert!(Program::compile(
        &[JumpIf {
            condition: 0,
            target: 0
        }],
        1,
        true
    )
    .is_err());
    let p = Program::compile(&[Return { src: 0 }], 1, false).unwrap();
    assert_eq!(p.run(&[1, 2], &mut [], 10), Err(Error::InvalidArguments));
}

#[test]
fn checked_little_endian_memory_and_overflow() {
    let p = Program::compile(
        &[
            Store64 {
                src: 0,
                address: 1,
                offset: 1,
            },
            Load64 {
                dst: 2,
                address: 1,
                offset: 1,
            },
            Return { src: 2 },
        ],
        3,
        true,
    )
    .unwrap();
    let value = 0x8877665544332211;
    let mut mem = [0xa5; 10];
    assert_eq!(p.run(&[value, 1], &mut mem, 3).unwrap().value, value);
    assert_eq!(&mem[2..], &value.to_le_bytes());
    for addr in [2, u64::MAX, u64::MAX - 1] {
        let mut memory = [0xa5; 10];
        assert_eq!(
            p.run(&[value, addr], &mut memory, 3),
            Err(Error::MemoryOutOfBounds)
        );
        assert_eq!(memory, [0xa5; 10]);
    }
}

#[test]
fn traps_and_memory_effects_are_ordered() {
    let p = Program::compile(
        &[
            Store64 {
                src: 0,
                address: 1,
                offset: 0,
            },
            DivSigned {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
            Return { src: 2 },
        ],
        3,
        true,
    )
    .unwrap();
    let mut memory = [0; 8];
    assert_eq!(p.run(&[5, 0], &mut memory, 3), Err(Error::DivisionByZero));
    assert_eq!(memory, 5u64.to_le_bytes());
    let d = Program::compile(
        &[
            DivSigned {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
            Return { src: 2 },
        ],
        3,
        true,
    )
    .unwrap();
    assert_eq!(
        d.run(&[i64::MIN as u64, (-1i64) as u64], &mut [], 2),
        Err(Error::IntegerOverflow)
    );
    assert_eq!(
        d.run(&[(-7i64) as u64, 2], &mut [], 2).unwrap().value as i64,
        -3
    );
}

#[test]
fn execution_state_is_fresh_and_program_can_be_shared() {
    let p = std::sync::Arc::new(
        Program::compile(&[Copy { dst: 1, src: 0 }, Return { src: 1 }], 2, true).unwrap(),
    );
    let workers: Vec<_> = (0..8)
        .map(|n| {
            let p = p.clone();
            std::thread::spawn(move || assert_eq!(p.run(&[n], &mut [], 2).unwrap().value, n))
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(p.run(&[], &mut [], 2).unwrap().value, 0);
}

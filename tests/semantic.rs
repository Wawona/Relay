use wwn_runtime::{Error, Op::*, Optimization, Program};

fn ops() -> [wwn_runtime::Op; 5] {
    [
        Add {
            dst: 0,
            lhs: 0,
            rhs: 1,
        },
        Mul {
            dst: 0,
            lhs: 0,
            rhs: 2,
        },
        Add {
            dst: 3,
            lhs: 3,
            rhs: 4,
        },
        JumpIf {
            condition: 3,
            target: 0,
        },
        Return { src: 0 },
    ]
}

#[test]
fn accelerated_loop_matches_reference_and_fuel() {
    let plain = Program::compile(&ops(), 5, false).unwrap();
    let fast = Program::compile_optimized(&ops(), 5, Optimization::Semantic).unwrap();
    assert!(fast.has_semantic_accelerator());
    let mut seed = 9371u64;
    for _ in 0..4096 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let n = (seed % 128) + 1;
        let args = [
            seed,
            seed.rotate_left(17),
            seed.rotate_right(13),
            n,
            u64::MAX,
        ];
        for fuel in [0, 4 * n - 1, 4 * n, 4 * n + 1, 4 * n + 2] {
            assert_eq!(
                fast.run(&args, &mut [], fuel),
                plain.run(&args, &mut [], fuel)
            );
        }
    }
}

#[test]
fn zero_start_wrong_step_and_cost_overflow_use_bounded_reference() {
    let p = Program::compile_optimized(&ops(), 5, Optimization::Semantic).unwrap();
    for (count, step) in [(0, u64::MAX), (5, 1), (u64::MAX, u64::MAX)] {
        assert_eq!(
            p.run(&[1, 2, 3, count, step], &mut [], 100),
            Err(Error::OutOfFuel)
        );
    }
}

#[test]
fn aliased_or_observable_loops_are_not_accelerated() {
    let mut aliased = ops();
    aliased[0] = Add {
        dst: 0,
        lhs: 0,
        rhs: 3,
    };
    let p = Program::compile_optimized(&aliased, 5, Optimization::Semantic).unwrap();
    assert!(!p.has_semantic_accelerator());
    let mut memory = ops();
    memory[0] = Store64 {
        src: 0,
        address: 1,
        offset: 0,
    };
    assert!(
        !Program::compile_optimized(&memory, 5, Optimization::Semantic)
            .unwrap()
            .has_semantic_accelerator()
    );
}

#[test]
fn return_can_observe_completed_counter() {
    let mut code = ops();
    code[4] = Return { src: 3 };
    let p = Program::compile_optimized(&code, 5, Optimization::Semantic).unwrap();
    let result = p.run(&[17, 0, 0, 1000, u64::MAX], &mut [], 4001).unwrap();
    assert_eq!(result.value, 0);
    assert_eq!(result.executed_ops, 4001);
}

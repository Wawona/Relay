#![cfg(feature = "wasm")]
use wwn_runtime::{wasm::Function, Error};

fn module(body: &[u8]) -> Vec<u8> {
    module_with_locals(body, &[0])
}
fn module_with_locals(body: &[u8], locals: &[u8]) -> Vec<u8> {
    assert!(body.len() < 120);
    let mut bytes = b"\0asm\x01\0\0\0\x01\x07\x01\x60\x02\x7e\x7e\x01\x7e\x03\x02\x01\x00\x07\x07\x01\x03run\x00\x00".to_vec();
    bytes.extend([
        10,
        (body.len() + locals.len() + 2) as u8,
        1,
        (body.len() + locals.len()) as u8,
    ]);
    bytes.extend(locals);
    bytes.extend(body);
    bytes
}
fn check(body: &[u8], cases: &[(u64, u64, u64)]) {
    check_module(&module(body), cases);
}
fn check_module(bytes: &[u8], cases: &[(u64, u64, u64)]) {
    for fusion in [false, true] {
        let f = Function::compile(bytes, "run", fusion).unwrap();
        for &(a, b, want) in cases {
            assert_eq!(f.run(&[a, b], 100_000).unwrap().value, want);
        }
    }
    #[cfg(feature = "wasmtime-baseline")]
    {
        let engine = wwn_runtime::baseline::engine().unwrap();
        let m = wasmtime::Module::new(&engine, bytes).unwrap();
        let mut store = wasmtime::Store::new(&engine, ());
        store.set_fuel(1_000_000).unwrap();
        let instance = wasmtime::Instance::new(&mut store, &m, &[]).unwrap();
        let f = instance
            .get_typed_func::<(i64, i64), i64>(&mut store, "run")
            .unwrap();
        for &(a, b, want) in cases {
            assert_eq!(
                f.call(&mut store, (a as i64, b as i64)).unwrap() as u64,
                want
            );
        }
    }
}
#[test]
fn if_else_merges_values() {
    check(
        &[
            0x20, 0, 0x50, 0x04, 0x7e, 0x42, 7, 0x05, 0x20, 1, 0x0b, 0x0b,
        ],
        &[(0, 99, 7), (1, 99, 99)],
    );
}
#[test]
fn loop_updates_locals_and_exits_nested_block() {
    // while a != 0: b += a; a -= 1. Branch exits two nested labels.
    check(
        &[
            0x02, 0x40, 0x03, 0x40, 0x20, 0, 0x50, 0x0d, 1, 0x20, 1, 0x20, 0, 0x7c, 0x21, 1, 0x20,
            0, 0x42, 1, 0x7d, 0x21, 0, 0x0c, 0, 0x0b, 0x0b, 0x20, 1, 0x0b,
        ],
        &[(0, 10, 10), (1, 10, 11), (100, 10, 5060)],
    );
}
#[test]
fn conditional_branch_preserves_fallthrough_value() {
    check(
        &[
            0x02, 0x7e, 0x20, 1, 0x20, 0, 0x50, 0x0d, 0, 0x42, 1, 0x7c, 0x0b, 0x0b,
        ],
        &[(0, 10, 10), (1, 10, 11)],
    );
}
#[test]
fn branch_to_function_and_explicit_return() {
    check(
        &[
            0x20, 1, 0x20, 0, 0x50, 0x0d, 0, 0x1a, 0x42, 9, 0x0f, 0x00, 0x0b,
        ],
        &[(0, 10, 10), (1, 10, 9)],
    );
}
#[test]
fn dead_nested_control_and_traps() {
    check(
        &[
            0x02, 0x7e, 0x42, 7, 0x0c, 0, 0x04, 0x40, 0x00, 0x05, 0x00, 0x0b, 0x0b, 0x0b,
        ],
        &[(0, 0, 7)],
    );
    let f = Function::compile(&module(&[0x00, 0x0b]), "run", true).unwrap();
    assert_eq!(f.run(&[0, 0], 10), Err(Error::Unreachable));
    let f = Function::compile(
        &module(&[0x03, 0x40, 0x0c, 0, 0x0b, 0x42, 0, 0x0b]),
        "run",
        true,
    )
    .unwrap();
    assert_eq!(f.run(&[0, 0], 10), Err(Error::OutOfFuel));
}
#[test]
fn integer_edges_match_wasm() {
    for (opcode, comparison, cases) in [
        (0x86, false, vec![(1, 64, 1), (1, 127, 1 << 63)]),
        (0x87, false, vec![(u64::MAX, 63, u64::MAX)]),
        (0x88, false, vec![(u64::MAX, 63, 1)]),
        (
            0x81,
            false,
            vec![(1 << 63, u64::MAX, 0), (u64::MAX, 2, u64::MAX)],
        ),
        (0x80, false, vec![(u64::MAX, 2, u64::MAX / 2)]),
        (0x53, true, vec![(u64::MAX, 0, 1), (0, u64::MAX, 0)]),
        (0x54, true, vec![(u64::MAX, 0, 0), (0, u64::MAX, 1)]),
    ] {
        let mut body = vec![0x20, 0, 0x20, 1, opcode];
        if comparison {
            body.push(0xad);
        }
        body.push(0x0b);
        check(&body, &cases);
    }
    for opcode in [0x7f, 0x80, 0x81, 0x82] {
        let f = Function::compile(&module(&[0x20, 0, 0x20, 1, opcode, 0x0b]), "run", true).unwrap();
        assert_eq!(f.run(&[1, 0], 10), Err(Error::DivisionByZero));
    }
}

#[test]
fn bit_counts_and_sign_extensions() {
    for (opcode, cases) in [
        (0x79, vec![(0, 0, 64), (1, 0, 63), (u64::MAX, 0, 0)]),
        (0x7a, vec![(0, 0, 64), (1 << 63, 0, 63), (3, 0, 0)]),
        (0x7b, vec![(0, 0, 0), (u64::MAX, 0, 64), (0x101, 0, 2)]),
        (0xc2, vec![(128, 0, (-128i64) as u64), (256, 0, 0)]),
        (0xc3, vec![(32768, 0, (-32768i64) as u64), (65536, 0, 0)]),
        (
            0xc4,
            vec![(1 << 31, 0, (-2147483648i64) as u64), (1 << 32, 0, 0)],
        ),
    ] {
        check(&[0x20, 0, opcode, 0x0b], &cases);
        check(&[0x02, 0x7e, 0x20, 0, opcode, 0x0b, 0x0b], &cases);
    }
}

#[test]
fn select_is_eager_and_preserves_stack_values() {
    for selection in [vec![0x1b], vec![0x1c, 1, 0x7e]] {
        let mut body = vec![0x20, 0, 0x20, 1, 0x20, 0, 0x50];
        body.extend(selection);
        body.push(0x0b);
        check(&body, &[(0, 7, 0), (8, 7, 7)]);
        let mut structured = vec![0x02, 0x7e];
        structured.extend(&body);
        structured.push(0x0b);
        check(&structured, &[(0, 7, 0), (8, 7, 7)]);
    }
    // Both operands execute even when the trapping value would be unselected.
    let f = Function::compile(
        &module(&[0x42, 7, 0x42, 1, 0x42, 0, 0x7f, 0x41, 1, 0x1b, 0x0b]),
        "run",
        true,
    )
    .unwrap();
    assert_eq!(f.run(&[0, 0], 100), Err(Error::DivisionByZero));
}

#[test]
fn i32_arithmetic_comparisons_and_shifts_match_pulley() {
    let edges = [
        0u32,
        1,
        2,
        31,
        32,
        63,
        64,
        127,
        255,
        32768,
        1 << 31,
        u32::MAX,
    ];
    for opcode in (0x6a..=0x78).chain(0x46..=0x4f) {
        let mut cases = Vec::new();
        for a in edges {
            for b in edges {
                let result = match opcode {
                    0x6a => Some(a.wrapping_add(b)),
                    0x6b => Some(a.wrapping_sub(b)),
                    0x6c => Some(a.wrapping_mul(b)),
                    0x6d => (a as i32).checked_div(b as i32).map(|v| v as u32),
                    0x6e => a.checked_div(b),
                    0x6f => {
                        if b == 0 {
                            None
                        } else {
                            Some((a as i32).wrapping_rem(b as i32) as u32)
                        }
                    }
                    0x70 => a.checked_rem(b),
                    0x71 => Some(a & b),
                    0x72 => Some(a | b),
                    0x73 => Some(a ^ b),
                    0x74 => Some(a.wrapping_shl(b)),
                    0x75 => Some((a as i32).wrapping_shr(b) as u32),
                    0x76 => Some(a.wrapping_shr(b)),
                    0x77 => Some(a.rotate_left(b)),
                    0x78 => Some(a.rotate_right(b)),
                    0x46 => Some(u32::from(a == b)),
                    0x47 => Some(u32::from(a != b)),
                    0x48 => Some(u32::from((a as i32) < (b as i32))),
                    0x49 => Some(u32::from(a < b)),
                    0x4a => Some(u32::from((a as i32) > (b as i32))),
                    0x4b => Some(u32::from(a > b)),
                    0x4c => Some(u32::from((a as i32) <= (b as i32))),
                    0x4d => Some(u32::from(a <= b)),
                    0x4e => Some(u32::from((a as i32) >= (b as i32))),
                    0x4f => Some(u32::from(a >= b)),
                    _ => unreachable!(),
                };
                if let Some(value) = result {
                    cases.push((a as u64, b as u64, value as u64));
                }
            }
        }
        let body = [0x20, 0, 0xa7, 0x20, 1, 0xa7, opcode, 0xad, 0x0b];
        check(&body, &cases);
        let mut structured = vec![0x02, 0x7e];
        structured.extend(body);
        structured.push(0x0b);
        check(&structured, &cases);
    }
}

#[test]
fn i32_unary_width_and_traps() {
    for (opcode, cases) in [
        (0x67, vec![(0, 0, 32), (1 << 32, 0, 32), (1, 0, 31)]),
        (0x68, vec![(0, 0, 32), (1 << 31, 0, 31)]),
        (0x69, vec![(u64::MAX, 0, 32)]),
        (0xc0, vec![(128, 0, 0xffffff80), (256, 0, 0)]),
        (0xc1, vec![(32768, 0, 0xffff8000), (65536, 0, 0)]),
    ] {
        check(&[0x20, 0, 0xa7, opcode, 0xad, 0x0b], &cases);
    }
    check(
        &[0x20, 0, 0xa7, 0xac, 0x0b],
        &[(1 << 31, 0, (-2147483648i64) as u64), (1 << 32, 0, 0)],
    );
    for opcode in 0x6d..=0x70 {
        let f = Function::compile(
            &module(&[0x20, 0, 0xa7, 0x20, 1, 0xa7, opcode, 0xad, 0x0b]),
            "run",
            true,
        )
        .unwrap();
        assert_eq!(f.run(&[1, 0], 100), Err(Error::DivisionByZero));
        if opcode == 0x6d {
            assert_eq!(
                f.run(&[1 << 31, u32::MAX as u64], 100),
                Err(Error::IntegerOverflow)
            );
        }
    }
    check(
        &[
            0x20, 0, 0xa7, 0x45, 0x04, 0x7e, 0x42, 7, 0x05, 0x42, 9, 0x0b, 0x0b,
        ],
        &[(1 << 32, 0, 7), (1, 0, 9)],
    );
}

#[test]
fn mutable_i32_local_survives_loop_backedges() {
    let body = [
        0x20, 0, 0xa7, 0x21, 2, 0x02, 0x40, 0x03, 0x40, 0x20, 2, 0x45, 0x0d, 1, 0x20, 1, 0x20, 2,
        0xad, 0x7c, 0x21, 1, 0x20, 2, 0x41, 1, 0x6b, 0x21, 2, 0x0c, 0, 0x0b, 0x0b, 0x20, 1, 0x0b,
    ];
    check_module(
        &module_with_locals(&body, &[1, 1, 0x7f]),
        &[(0, 7, 7), (100, 7, 5057), (1 << 32, 7, 7)],
    );
    check_module(
        &module_with_locals(&[0x20, 2, 0xad, 0x0b], &[1, 1, 0x7f]),
        &[(99, 88, 0)],
    );
}

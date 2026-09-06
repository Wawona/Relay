#![cfg(feature = "wasm")]
use wwn_runtime::{wasm::Function, Error};

fn module(body: &[u8]) -> Vec<u8> {
    assert!(body.len() < 120);
    let mut bytes = b"\0asm\x01\0\0\0\x01\x07\x01\x60\x02\x7e\x7e\x01\x7e\x03\x02\x01\x00\x07\x07\x01\x03run\x00\x00".to_vec();
    bytes.extend([10, (body.len() + 3) as u8, 1, (body.len() + 1) as u8, 0]);
    bytes.extend(body);
    bytes
}
fn check(body: &[u8], cases: &[(u64, u64, u64)]) {
    let bytes = module(body);
    for fusion in [false, true] {
        let f = Function::compile(&bytes, "run", fusion).unwrap();
        for &(a, b, want) in cases {
            assert_eq!(f.run(&[a, b], 100_000).unwrap().value, want);
        }
    }
    #[cfg(feature = "wasmtime-baseline")]
    {
        let engine = wwn_runtime::baseline::engine().unwrap();
        let m = wasmtime::Module::new(&engine, &bytes).unwrap();
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

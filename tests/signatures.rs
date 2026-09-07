#![cfg(feature = "wasm")]
use wwn_runtime::{
    wasm::{Function, IntegerType},
    Error,
};

fn module(params: &[u8], result: u8, body: &[u8]) -> Vec<u8> {
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    let mut ty = vec![1, 0x60, params.len() as u8];
    ty.extend(params);
    ty.extend([1, result]);
    bytes.extend([1, ty.len() as u8]);
    bytes.extend(ty);
    bytes.extend([3, 2, 1, 0, 7, 7, 1, 3, b'r', b'u', b'n', 0, 0]);
    bytes.extend([10, (body.len() + 3) as u8, 1, (body.len() + 1) as u8, 0]);
    bytes.extend(body);
    bytes
}

#[test]
fn i32_export_normalizes_arguments_and_returns_bits() {
    for body in [
        vec![0x20, 0, 0x0b],
        vec![0x02, 0x7f, 0x20, 0, 0x0c, 0, 0x0b, 0x0b],
    ] {
        let bytes = module(&[0x7f], 0x7f, &body);
        for fusion in [false, true] {
            let f = Function::compile(&bytes, "run", fusion).unwrap();
            assert_eq!(f.parameter_types(), &[IntegerType::I32]);
            assert_eq!(f.result_type(), IntegerType::I32);
            for arg in [0, 1, u32::MAX as u64, 1 << 32, u64::MAX] {
                assert_eq!(f.run(&[arg], 100).unwrap().value, arg as u32 as u64);
            }
            assert_eq!(f.run(&[], 100), Err(Error::InvalidArguments));
            assert_eq!(f.run(&[0], 0), Err(Error::OutOfFuel));
        }
    }
}

#[test]
fn mixed_signature_preserves_i64_and_extends_i32_signed() {
    // (i32, i64) -> i64: sign-extend first parameter then add second.
    for body in [
        vec![0x20, 0, 0xac, 0x20, 1, 0x7c, 0x0b],
        vec![0x02, 0x7e, 0x20, 0, 0xac, 0x20, 1, 0x7c, 0x0b, 0x0b],
    ] {
        let bytes = module(&[0x7f, 0x7e], 0x7e, &body);
        let cases = [(u64::MAX, 5), (1 << 32, u64::MAX), (0x80000000, 1 << 40)];
        for fusion in [false, true] {
            let f = Function::compile(&bytes, "run", fusion).unwrap();
            assert_eq!(f.parameter_types(), &[IntegerType::I32, IntegerType::I64]);
            assert_eq!(f.result_type(), IntegerType::I64);
            for (a, b) in cases {
                let expected = (a as i32 as i64 as u64).wrapping_add(b);
                assert_eq!(f.run(&[a, b], 100).unwrap().value, expected);
            }
        }
        #[cfg(feature = "wasmtime-baseline")]
        {
            let engine = wwn_runtime::baseline::engine().unwrap();
            let module = wasmtime::Module::new(&engine, &bytes).unwrap();
            let mut store = wasmtime::Store::new(&engine, ());
            store.set_fuel(10_000).unwrap();
            let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
            let reference = instance
                .get_typed_func::<(i32, i64), i64>(&mut store, "run")
                .unwrap();
            let f = Function::compile(&bytes, "run", true).unwrap();
            for (a, b) in cases {
                assert_eq!(
                    f.run(&[a, b], 100).unwrap().value,
                    reference.call(&mut store, (a as i32, b as i64)).unwrap() as u64
                );
            }
        }
    }
}

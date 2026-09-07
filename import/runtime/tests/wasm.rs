#![cfg(feature = "wasm")]
use wwn_runtime::{wasm::Function, Error};

fn module(params: u8, body: &[u8]) -> Vec<u8> {
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    let mut ty = vec![1, 0x60, params];
    ty.extend(std::iter::repeat_n(0x7e, params as usize));
    ty.extend([1, 0x7e]);
    bytes.extend([1, ty.len() as u8]);
    bytes.extend(ty);
    bytes.extend([3, 2, 1, 0]);
    bytes.extend([7, 7, 1, 3, b'r', b'u', b'n', 0, 0]);
    bytes.extend([10, (body.len() + 3) as u8, 1, (body.len() + 1) as u8, 0]);
    bytes.extend(body);
    bytes
}

#[test]
fn executes_real_wasm_and_fuses_ssa_arithmetic() {
    let bytes = module(3, &[0x20, 0, 0x20, 1, 0x7c, 0x20, 2, 0x7e, 0x0b]);
    for fusion in [false, true] {
        let function = Function::compile(&bytes, "run", fusion).unwrap();
        assert_eq!(function.fused_pairs(), usize::from(fusion));
        assert_eq!(function.run(&[5, 7, 3], 3).unwrap().value, 36);
        assert_eq!(function.run(&[5, 7, 3], 2), Err(Error::OutOfFuel));
        assert_eq!(function.run(&[5, 7], 3), Err(Error::InvalidArguments));
    }
}

#[test]
fn local_set_does_not_change_existing_stack_value() {
    // get 0; const 5; set 0; get 0; add. Original get remains the old value.
    let bytes = module(1, &[0x20, 0, 0x42, 5, 0x21, 0, 0x20, 0, 0x7c, 0x0b]);
    assert_eq!(
        Function::compile(&bytes, "run", true)
            .unwrap()
            .run(&[7], 3)
            .unwrap()
            .value,
        12
    );
}

#[test]
fn invalid_modules_and_unsupported_features_do_not_execute() {
    let bytes = module(1, &[0x20, 0, 0x0b]);
    for end in 0..bytes.len() {
        assert!(Function::compile(&bytes[..end], "run", true).is_err());
    }
    assert!(Function::compile(&bytes, "missing", true).is_err());
    // f64.convert_i64_s followed by truncation is valid but unsupported.
    let unsupported = module(1, &[0x20, 0, 0xb9, 0xb0, 0x0b]);
    assert!(Function::compile(&unsupported, "run", true).is_err());
}

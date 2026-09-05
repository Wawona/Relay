#![cfg(all(feature = "wasm", feature = "wasmtime-baseline"))]
use wasmtime::{Instance, Module, Store};

#[test]
fn static_matches_pulley_on_real_wasm() -> wasmtime::Result<()> {
    let bytes = b"\0asm\x01\0\0\0\x01\x08\x01\x60\x03\x7e\x7e\x7e\x01\x7e\x03\x02\x01\x00\x07\x07\x01\x03run\x00\x00\x0a\x0c\x01\x0a\x00\x20\x00\x20\x01\x7c\x20\x02\x7e\x0b";
    let engine = wwn_runtime::baseline::engine()?;
    let module = Module::new(&engine, bytes)?;
    let mut store = Store::new(&engine, ());
    store.set_fuel(10_000_000)?;
    let instance = Instance::new(&mut store, &module, &[])?;
    let reference = instance.get_typed_func::<(i64, i64, i64), i64>(&mut store, "run")?;
    let static_fn = wwn_runtime::wasm::Function::compile(bytes, "run", true)?;
    let mut state = 0xd12ea7a15eed1234u64;
    for _ in 0..10000 {
        let mut args = [0; 3];
        for v in &mut args {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *v = state;
        }
        let expected =
            reference.call(&mut store, (args[0] as i64, args[1] as i64, args[2] as i64))?;
        assert_eq!(static_fn.run(&args, 3)?.value as i64, expected);
    }
    Ok(())
}

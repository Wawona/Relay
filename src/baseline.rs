//! Complete-WASM reference engine. Pulley provides the mature execution path
//! while the experimental static frontend gains coverage. This is upstream
//! Wasmtime execution, not a claim that Wawona's new optimizer handles WASI.

/// The target is fixed before engine construction. Cranelift translates WASM
/// into Pulley bytecode; generated guest programs remain data on every host.
/// WASI P1/P2 linkers remain owned by wwn-wasm.
pub fn engine() -> wasmtime::Result<wasmtime::Engine> {
    let mut config = wasmtime::Config::new();
    config.target("pulley64")?;
    config.wasm_component_model(true);
    config.consume_fuel(true);
    wasmtime::Engine::new(&config)
}

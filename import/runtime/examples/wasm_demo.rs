//! Actual WASM bytes for (a + b) * c, compiled to static-handler IR at runtime.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = b"\0asm\x01\0\0\0\x01\x08\x01\x60\x03\x7e\x7e\x7e\x01\x7e\x03\x02\x01\x00\x07\x07\x01\x03run\x00\x00\x0a\x0c\x01\x0a\x00\x20\x00\x20\x01\x7c\x20\x02\x7e\x0b";
    let function = wwn_runtime::wasm::Function::compile(bytes, "run", true)?;
    let result = function.run(&[5, 7, 3], 3)?;
    assert_eq!(result.value, 36);
    assert_eq!(function.fused_pairs(), 1);
    println!("{{\"demo\":\"wasm-static-handlers\",\"host_os\":\"{}\",\"host_arch\":\"{}\",\"value\":{},\"fused_pairs\":{}}}",
        std::env::consts::OS, std::env::consts::ARCH, result.value, function.fused_pairs());
    Ok(())
}

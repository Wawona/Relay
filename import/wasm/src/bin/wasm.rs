//! Host CLI (`wasm <file.wasm> [args…]`) for macOS / Linux / Android.
//! Apple mobile uses the C ABI from zsh dispatch instead.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match wawona_wasm::run_args(&args) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("wawona-wasm: {e:#}");
            std::process::exit(127);
        }
    }
}

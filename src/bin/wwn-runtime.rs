use std::process::ExitCode;
#[cfg(feature = "wasm")]
use std::{env, fs::File, io::Read};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wwn-runtime: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "wasm")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: wwn-runtime FILE.wasm EXPORT [i64 arguments...]")?;
    let export = args.next().ok_or("missing export name")?;
    let arguments = args
        .map(|s| s.parse::<i64>().map(|v| v as u64))
        .collect::<Result<Vec<_>, _>>()?;
    // Bound actual bytes read, including files that grow after opening.
    let mut bytes = Vec::new();
    File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("module byte limit".into());
    }
    let function = wwn_runtime::wasm::Function::compile(&bytes, &export, true)?;
    let result = function.run(&arguments, 10_000_000)?;
    println!("{}", result.value as i64);
    Ok(())
}

#[cfg(not(feature = "wasm"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    Err("WASM frontend feature is disabled".into())
}

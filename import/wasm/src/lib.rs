//! Wawona WASI P1/P2 runtime — interpreter on Apple mobile (Pulley),
//! Cranelift optional on macOS only.

pub mod ffi;
pub mod host;
pub mod interrupt;
pub mod p1;
pub mod p2;
pub mod sandbox;

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::sync::OnceLock;
use wasmtime::{Config, Engine};

/// Process-wide Wasmtime engine. Mode A Pulley cold-starts must reuse this
/// across Machines Start / shell `wasm` (and Module/Linker caches below).
fn shared_engine() -> Result<&'static Engine> {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }
    let engine = build_engine()?;
    Ok(ENGINE.get_or_init(|| engine))
}

/// True when `engine` is the process-wide Mode A engine (safe for Module/Linker cache).
pub fn is_shared_engine(engine: &Engine) -> bool {
    shared_engine()
        .map(|shared| std::ptr::eq(engine, shared))
        .unwrap_or(false)
}

fn build_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    config.epoch_interruption(true);
    // Apple mobile / default: Pulley interpreter. Cranelift native is a
    // macOS-only feature and must never be compiled into iphoneos slices.
    #[cfg(feature = "pulley")]
    {
        config.target("pulley64").context("Config::target(pulley64)")?;
    }
    #[cfg(all(feature = "cranelift-native", not(feature = "pulley")))]
    {
        // Host Cranelift (macOS). No extra target.
    }
    Engine::new(&config).context("wasmtime Engine")
}

/// Backend label for benches and logs. Mode A Apple mobile is always Pulley.
pub fn backend_name() -> &'static str {
    #[cfg(feature = "pulley")]
    {
        "pulley64"
    }
    #[cfg(all(feature = "cranelift-native", not(feature = "pulley")))]
    {
        "cranelift-native"
    }
    #[cfg(not(any(
        feature = "pulley",
        all(feature = "cranelift-native", not(feature = "pulley"))
    )))]
    {
        "unknown"
    }
}

pub fn run_args(args: &[String]) -> Result<i32> {
    let path = ffi::path_from_c_args(args).context("usage: wasm <file.wasm|package> [args…]")?;
    let path = resolve_wasm_path(&path)?;
    if !path.is_file() {
        bail!("not a file: {}", path.display());
    }
    if !sandbox::is_wasm_magic(&path) {
        bail!("{} is not a WASM module (missing \\0asm magic)", path.display());
    }
    let guest_args = if args
        .first()
        .map(|s| s == "wasm" || s.ends_with("/wasm"))
        .unwrap_or(false)
    {
        args[1..].to_vec()
    } else {
        args.to_vec()
    };
    run_path(&path, &guest_args)
}

/// Resolve `./file.wasm`, absolute paths, or an installed `wpm` package name.
fn resolve_wasm_path(path: &Path) -> Result<std::path::PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or(""));
    if name.is_empty() || name.contains('.') && !name.ends_with(".wasm") {
        bail!("not a file: {}", path.display());
    }
    // Package names are bare identifiers (no slash).
    if path.components().count() == 1 {
        if let Ok(store) = wpm::PackageStore::open_default() {
            if let Ok(p) = store.resolve_wasm(name) {
                return Ok(p);
            }
        }
    }
    bail!("not a file or installed package: {}", path.display())
}

pub fn run_path(path: &Path, args: &[String]) -> Result<i32> {
    let engine = shared_engine()?;
    run_path_with_engine(engine, path, args)
}

/// Same as [`run_path`], but uses a caller-owned engine (benches / cold path).
pub fn run_path_with_engine(engine: &Engine, path: &Path, args: &[String]) -> Result<i32> {
    if sandbox::looks_like_component(path) {
        match p2::run(engine, path, args) {
            Ok(code) => return Ok(code),
            Err(e) => {
                // Fall back to P1 if the component parser rejected a core module
                // that happened to have version 0x0d in the reserved field.
                eprintln!("wawona-wasm: P2 load failed ({e:#}); trying P1");
            }
        }
    }
    p1::run(engine, path, args)
}

/// Build a fresh engine (cold-start baseline for Mode A benches).
pub fn new_engine_for_bench() -> Result<Engine> {
    build_engine()
}

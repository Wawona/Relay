//! WASI Preview 1 (`wasm32-wasip1` / `wasi_snapshot_preview1`).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::p2::WasiCtxBuilder;
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{DirPerms, FilePerms, I32Exit};

use crate::sandbox;

pub struct P1State {
    pub wasi: WasiP1Ctx,
}

struct CachedModule {
    path: PathBuf,
    modified: SystemTime,
    len: u64,
    module: Module,
}

fn module_cache() -> &'static Mutex<Option<CachedModule>> {
    static CACHE: OnceLock<Mutex<Option<CachedModule>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn build_linker(engine: &Engine) -> Result<Linker<P1State>> {
    let mut linker: Linker<P1State> = Linker::new(engine);
    preview1::add_to_linker_sync(&mut linker, |s: &mut P1State| &mut s.wasi)
        .context("link wasi_snapshot_preview1")?;
    crate::host::add_host_imports(&mut linker).context("link wawona host ABI")?;
    Ok(linker)
}

fn shared_linker(engine: &Engine) -> Result<&'static Linker<P1State>> {
    static LINKER: OnceLock<Linker<P1State>> = OnceLock::new();
    if let Some(linker) = LINKER.get() {
        return Ok(linker);
    }
    let linker = build_linker(engine)?;
    Ok(LINKER.get_or_init(|| linker))
}

fn load_module(engine: &Engine, path: &Path) -> Result<Module> {
    let meta = std::fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let len = meta.len();
    let cacheable = crate::is_shared_engine(engine);
    if cacheable {
        let guard = module_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = guard.as_ref() {
            if cached.path == path && cached.modified == modified && cached.len == len {
                return Ok(cached.module.clone());
            }
        }
    }
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let module = Module::new(engine, &bytes).context("load WASI P1 module")?;
    if cacheable {
        if let Ok(mut guard) = module_cache().lock() {
            *guard = Some(CachedModule {
                path: path.to_path_buf(),
                modified,
                len,
                module: module.clone(),
            });
        }
    }
    Ok(module)
}

pub fn run(engine: &Engine, path: &std::path::Path, args: &[String]) -> Result<i32> {
    let module = load_module(engine, path)?;
    // Linker is Engine-scoped. Cache only for the process-wide Mode A engine.
    let owned_linker;
    let linker: &Linker<P1State> = if crate::is_shared_engine(engine) {
        shared_linker(engine)?
    } else {
        owned_linker = build_linker(engine)?;
        &owned_linker
    };

    let root = sandbox::sandbox_root();
    let mut builder = WasiCtxBuilder::new();
    // iOS/tvOS/watchOS in-process shell wires host STDOUT to the PTY and host
    // STDERR to the app log (so NSLog does not spam weston-terminal). Guest
    // CLI usage/errors go to stderr by convention; mirror them onto stdout
    // when the fake-TTY shell is active so they are visible.
    builder.inherit_stdin();
    builder.inherit_stdout();
    if std::env::var_os("WAWONA_PTY_FAKE_TTY").is_some() {
        builder.stderr(wasmtime_wasi::p2::stdout());
    } else {
        builder.inherit_stderr();
    }
    builder.inherit_env();
    builder.args(args);
    builder.env("HOME", "/");
    builder.env("PWD", "/");
    builder.env("USER", "mobile");
    builder
        .preopened_dir(&root, "/", DirPerms::all(), FilePerms::all())
        .with_context(|| format!("preopen {}", root.display()))?;

    let mut store = Store::new(
        engine,
        P1State {
            wasi: builder.build_p1(),
        },
    );
    store
        .set_fuel(crate::sandbox::fuel_budget())
        .ok();
    let _session = crate::interrupt::RunSession::begin(engine, &mut store);

    let instance = linker
        .instantiate(&mut store, &module)
        .context("instantiate P1")?;
    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .context("missing _start")?;
    let result = match start.call(&mut store, ()) {
        Ok(()) => Ok(0),
        Err(e) => {
            if let Some(code) = e.downcast_ref::<I32Exit>() {
                return crate::interrupt::map_run_result(Ok(code.0));
            }
            Err(e).context("P1 _start")
        }
    };
    crate::interrupt::map_run_result(result)
}

/// Drop cached Module (benches / tests).
pub fn clear_module_cache_for_bench() {
    if let Ok(mut guard) = module_cache().lock() {
        *guard = None;
    }
}

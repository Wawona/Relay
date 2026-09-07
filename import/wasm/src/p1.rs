//! WASI Preview 1 (`wasm32-wasip1` / `wasi_snapshot_preview1`).

use anyhow::{Context, Result};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::p2::WasiCtxBuilder;
use wasmtime_wasi::{DirPerms, FilePerms};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::I32Exit;

use crate::sandbox;

pub struct P1State {
    pub wasi: WasiP1Ctx,
}

pub fn run(engine: &Engine, path: &std::path::Path, args: &[String]) -> Result<i32> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let module = Module::new(engine, &bytes).context("load WASI P1 module")?;
    let mut linker: Linker<P1State> = Linker::new(engine);
    preview1::add_to_linker_sync(&mut linker, |s: &mut P1State| &mut s.wasi)
        .context("link wasi_snapshot_preview1")?;
    crate::host::add_host_imports(&mut linker).context("link wawona host ABI")?;

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
    store.set_fuel(25_000_000).ok();

    let instance = linker
        .instantiate(&mut store, &module)
        .context("instantiate P1")?;
    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .context("missing _start")?;
    match start.call(&mut store, ()) {
        Ok(()) => Ok(0),
        Err(e) => {
            if let Some(code) = e.downcast_ref::<I32Exit>() {
                return Ok(code.0);
            }
            Err(e).context("P1 _start")
        }
    }
}

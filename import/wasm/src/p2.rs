//! WASI Preview 2 component (`wasm32-wasip2`).

use anyhow::{Context, Result};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::p2::{
    add_to_linker_sync, IoView, WasiCtx, WasiCtxBuilder, WasiView,
};
use wasmtime_wasi::{DirPerms, FilePerms};

use crate::sandbox;

struct P2State {
    table: ResourceTable,
    wasi: WasiCtx,
}

impl IoView for P2State {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl WasiView for P2State {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

pub fn run(engine: &Engine, path: &std::path::Path, args: &[String]) -> Result<i32> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let component = Component::new(engine, &bytes).context("load WASI P2 component")?;
    let mut linker = Linker::new(engine);
    add_to_linker_sync(&mut linker).context("link WASI 0.2")?;

    let root = sandbox::sandbox_root();
    let mut builder = WasiCtxBuilder::new();
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
    let _ = builder.preopened_dir(&root, "/", DirPerms::all(), FilePerms::all());

    let mut store = Store::new(
        engine,
        P2State {
            table: ResourceTable::new(),
            wasi: builder.build(),
        },
    );
    store.set_fuel(25_000_000).ok();

    let cmd = wasmtime_wasi::p2::bindings::sync::Command::instantiate(
        &mut store,
        &component,
        &linker,
    )
    .context("instantiate wasi:cli/command")?;
    match cmd.wasi_cli_run().call_run(&mut store) {
        Ok(Ok(())) => Ok(0),
        Ok(Err(())) => Ok(1),
        Err(e) => Err(e).context("P2 run"),
    }
}

//! Mode A WASI microbench: cold Engine vs shared Engine+Module (Pulley).
//!
//! Product runtime is this crate (`import/wasm`). Relay execute must reuse
//! the same Pulley path (`wawona_wasm_run` / `run_path`). Never ship a second
//! Mode A interpreter that loses to this baseline.
//!
//! Usage:
//!   cargo run -p wawona-wasm --bin wasm_bench --release -- /path/to/module.wasm [iters]
//!
//! Env:
//!   WAWONA_WASM_FUEL   fuel budget (default 25_000_000)

use std::env;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let mut args = env::args().skip(1);
    let module = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: wasm_bench MODULE.wasm [iters]");
            eprintln!("backend={}", wawona_wasm::backend_name());
            std::process::exit(2);
        }
    };
    let iters: u32 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);
    if !module.is_file() {
        eprintln!("not a file: {}", module.display());
        std::process::exit(2);
    }

    let guest_args = vec![
        "wasm".into(),
        module.display().to_string(),
        "hello".into(),
    ];

    println!(
        "wawona-wasm Mode A bench backend={} module={} iters={}",
        wawona_wasm::backend_name(),
        module.display(),
        iters
    );

    // Cold: new Engine + compile Module every run (pre-cache baseline).
    wawona_wasm::p1::clear_module_cache_for_bench();
    let mut cold_ns = 0u128;
    for _ in 0..iters {
        let start = Instant::now();
        let engine = wawona_wasm::new_engine_for_bench().expect("cold engine");
        let _ = wawona_wasm::run_path_with_engine(&engine, &module, &guest_args).unwrap_or_else(
            |error| {
                eprintln!("cold run failed: {error:#}");
                std::process::exit(1);
            },
        );
        cold_ns += start.elapsed().as_nanos();
    }

    // Warm shared Engine, but force Module recompile each iter.
    let _ = wawona_wasm::run_path(&module, &guest_args); // prime Engine + Linker
    let mut warm_engine_ns = 0u128;
    for _ in 0..iters {
        wawona_wasm::p1::clear_module_cache_for_bench();
        let start = Instant::now();
        let _ = wawona_wasm::run_path(&module, &guest_args).unwrap_or_else(|error| {
            eprintln!("warm-engine run failed: {error:#}");
            std::process::exit(1);
        });
        warm_engine_ns += start.elapsed().as_nanos();
    }

    // Product path: shared Engine + cached Module + cached Linker.
    let _ = wawona_wasm::run_path(&module, &guest_args); // prime Module cache
    let mut warm_ns = 0u128;
    for _ in 0..iters {
        let start = Instant::now();
        let _ = wawona_wasm::run_path(&module, &guest_args).unwrap_or_else(|error| {
            eprintln!("warm-module run failed: {error:#}");
            std::process::exit(1);
        });
        warm_ns += start.elapsed().as_nanos();
    }

    let cold_ms = (cold_ns as f64) / 1_000_000.0 / (iters as f64);
    let warm_engine_ms = (warm_engine_ns as f64) / 1_000_000.0 / (iters as f64);
    let warm_ms = (warm_ns as f64) / 1_000_000.0 / (iters as f64);
    let vs_cold = if warm_ms > 0.0 {
        cold_ms / warm_ms
    } else {
        f64::INFINITY
    };

    println!("cold_engine_ms_per_run={cold_ms:.3}");
    println!("warm_engine_only_ms_per_run={warm_engine_ms:.3}");
    println!("warm_engine_module_ms_per_run={warm_ms:.3}");
    println!("warm_vs_cold_speedup={vs_cold:.2}x");
    println!("gate=ModeA_Pulley_cached_path_must_beat_cold");
    if warm_ms > cold_ms {
        eprintln!("FAIL: product warm path slower than cold Engine baseline");
        std::process::exit(1);
    }
    println!("PASS");
}

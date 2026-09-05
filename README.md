# Wawona Runtime

[![Runtime CI](https://github.com/Wawona/wwn-runtime/actions/workflows/runtime.yml/badge.svg)](https://github.com/Wawona/wwn-runtime/actions/workflows/runtime.yml)

Research implementation of an optimizing runtime whose translated programs are
data referencing native handlers compiled into the signed application.

**Target:** the world's fastest App Store compliant iOS runtime for WASM,
WASI P1/P2, QEMU virtual machines, and VM-backed containers, including beating
UTM with JIT on jailbroken iOS on a declared, equivalent workload suite.
This is a target, not a measured result or an App Store approval claim.

## Current implementation

- Validated register IR, bounded branches and registers, wrapping 64-bit integer
  arithmetic, signed-division traps, checked little-endian memory access.
- Locally selected static function handlers; no unsafe Rust in the engine.
- Add/multiply fusion preserving aliasing, branch entries, and IR fuel accounting.
- Guarded affine-loop acceleration using modular exponentiation. Only pure,
  recognized loops qualify; guards retain the original bounded execution path.
- Experimental pure i64 WASM function frontend using full module validation,
  SSA locals, and stack elimination. Handles constants, locals, add, multiply,
  signed division, drop, return, and end. Other features fail before execution.
- Differential fusion tests, memory/trap tests, concurrent invocation tests,
  real binary WASM tests, and a dispatch microbenchmark with raw samples.
- Optional shared Pulley reference engine, fixed to bytecode translation.
  The sibling wwn-wasm research branch consumes it for WASI P1/P2 and exposes
  supported custom exports through `wasm --static-run FILE EXPORT [i64...]`.

The initial dispatcher calls handlers in a loop. It is not yet tail-threaded,
register-pinned, profile-generated, or a full WASM/QEMU implementation. Existing
`wwn-wasm` Pulley/WASI P1/P2 and `wwn-vms` QEMU remain the working reference
backends. The new engine is not yet linked into Wawona.

## Run

```sh
cargo test --locked
cargo test --locked --features wasmtime-baseline --test reference
cargo run --release --example static_demo
cargo run --release --example dispatch_bench
cargo run --release -- FILE.wasm EXPORT 5 7 3
cargo build --release --target aarch64-apple-ios
cargo build --release --target aarch64-apple-ios-sim
```

CLI inputs are signed i64 values; engine values are their u64 bit patterns.
Fuel currently counts lowered IR operations, not WASM source instructions.
The microbenchmark compares fused/unfused dispatch, a specialized affine-loop
accelerator, and a native iterative reference. The accelerator changes the
algorithmic complexity for this recognized recurrence only. Its timings include
timer overhead; they are not a general-purpose dispatch speed measurement.
It cannot substantiate a comparison with UTM, containers, or WASI.

## Architecture and ownership

The intended dependency graph puts `wwn-runtime` below `wwn-wasm` and `wwn-vms`.
`wwn-containers` consumes the VM engine for iOS OCI execution. Wawona integrates
all three in Machines. No dependency points back from a runtime into Wawona.
Only the local Cargo integration in wwn-wasm exists so far. Nix source wiring,
QEMU integration, container integration, and Wawona app linking remain pending.
Native bundled Weston/Niri remain native and preserve upstream behavior.

Runtime-created operands, branch metadata, optimized IR, and lookup tables must
stay non-executable data. Persisted translations must use validated opcode IDs,
never serialized process pointers. Host APIs stay behind sandboxed WASI or
guest-kernel boundaries. This technical invariant does not by itself establish
App Store policy compliance.

See [development plan](docs/development.md) for integration and release gates.
See [CI scope and reproduction](docs/ci.md) for exactly what each runner verifies.

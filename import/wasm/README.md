# wwn-wasm — Wawona Runtime (WASI P1/P2)

[![CI](https://github.com/Wawona/wwn-wasm/actions/workflows/ci.yml/badge.svg?branch=development)](https://github.com/Wawona/wwn-wasm/actions/workflows/ci.yml)
[![Wawona Gate: wasm-wayland](https://github.com/Wawona/Wawona/actions/workflows/wasm-wayland.yml/badge.svg?branch=development)](https://github.com/Wawona/Wawona/actions/workflows/wasm-wayland.yml)
[![Milestone](https://img.shields.io/badge/milestone-WASI_P1%2FP2-blue)](https://github.com/Wawona/Wawona/milestone/2)

In-process **WASI Preview 1 + Preview 2** runtime for Wawona (the **Wawona
Runtime**).

Apple mobile uses Wasmtime **Pulley** (interpreter: WASM → Pulley IR → interpret;
no native ARM, no `MAP_JIT`). macOS may use Cranelift. `.wasm` files are user
documents — Apple does not sign them.

This is an L3′ repo: depends on `wwn-toolchain` only. See
[wwn-repo-dag.md](https://github.com/Wawona/Wawona/blob/development/docs/wwn-repo-dag.md)
and milestone
[Support WASI P1 P2 WASM!](https://github.com/Wawona/Wawona/milestone/2).

C ABI (`include/wawona_wasm.h`):

- `wawona_wasm_can_run(path)`
- `wawona_wasm_run(argc, argv)`
- `wawona_terminal_raw_enabled()`

Host extras (P1 import module `wawona_socket` / `wawona_terminal`):

- POSIX TCP/UDP/unix + `connect_host`
- `wawona_wayland_connect` / `shm_create` / `shm_write` / `sendmsg`
  (SCM_RIGHTS into the existing compositor)
- TTY raw/cooked bit

P2 guests use `wasi:cli` / `filesystem` / `sockets` / `clocks` / `random`.
`wasi:http` is not linked yet (size); use sockets or a native port.

Native ports stay first-class. WASM is the long-tail escape hatch.

## Package manager (`wpm`)

Sibling crate [`crates/wpm`](crates/wpm). Mode A Runtime packages for **all**
targets (shell + Files sideload + [`repo.wawona.io/wasm/v1`](https://repo.wawona.io/wasm/v1)):

```text
wpm install ./tool.wasm     # local / Files.app
wpm install hello           # https://repo.wawona.io/wasm/v1
wpm list | search | remove
# web catalog (same index): https://repo.wawona.io/search/?channel=wasm
wasm hello                  # Runtime resolves installed package names
```

C ABI: `wpm_main` (weak in `wawona-dispatch`). Jailbreak `.deb` APT is a
different channel. `wpm` refuses those URLs.

Local smoke (Go or rustup):

```bash
./.github/scripts/smoke-runtime-wpm.sh
```

Plan: [wasm-package-manager.md](https://github.com/Wawona/Wawona/blob/development/docs/wasm-package-manager.md).

## CI

| Check | Where |
|---|---|
| Pulley `cargo check` / sandbox tests / iOS recipe guard | this repo `ci.yml` |
| Wayland SHM guest build + Weston headless smoke | this repo `ci.yml` (`wayland-smoke`) |
| Wawona flake wiring + runtime↔compositor smoke | [Wawona **Gate: wasm-wayland**](https://github.com/Wawona/Wawona/actions/workflows/wasm-wayland.yml) |

```bash
# Local Wayland smoke (needs weston + rustup):
./.github/scripts/smoke-wayland-shm.sh
```

Demos: [`examples/`](examples/README.md). The Wayland SHM client
(`examples/wayland-shm`) compiles from **Rust, Go, or Swift** with that
language’s toolchain — no Nix.

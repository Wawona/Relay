# WASI demos for the Wawona runtime

Compile on a desktop with **that language’s toolchain** (not Nix). Copy the
`.wasm` into the Wawona Documents folder (Files / File Sharing / `scp`), then:

```text
wasm ./tool.wasm hello
./tool.wasm hello
wasm ./wayland-shm-rust.wasm
```

| Dir | Toolchain (no Nix) | What it is |
|---|---|---|
| `rust/` | `rustup` + `cargo` / `wasm32-wasip1` | CLI: hello, fs-*, tcp-client |
| `go/` | Go 1.21+ `GOOS=wasip1 GOARCH=wasm` | CLI: hello, fs-* |
| `swift/` | Swift 6.2+ + wasm SDK, **no Foundation** | CLI: hello |
| `wasip2/` | `rustup` + `cargo` / `wasm32-wasip2` | `wasi:cli` hello |
| **`hello-wasi-gui/`** | rust / `wasm32-wasip1` | Minimal Wayland hello (`wl_shm` + xdg; no seat) |
| **`wayland-shm/`** | rust **or** go **or** swift | **Real Wayland client** (`wl_shm` + xdg; Rust: seat + soft UI) |

```bash
# Wayland hello (smoke) and interactive demo
./hello-wasi-gui/rust/build.sh
./wayland-shm/rust/build.sh
./wayland-shm/go/build.sh
./wayland-shm/swift/build.sh
```

Native ports stay first-class. Prefer `weston-simple-shm` / `foot` when we
have a recipe. See `Wawona/docs/wasm-wasi.md`.

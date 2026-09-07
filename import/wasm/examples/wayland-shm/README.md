# wayland-shm — Wawona runtime WASM client

Same **Wayland** client family in Rust, Go, and Swift: `wl_compositor` +
`xdg_wm_base` + `wl_shm`. Speaks the real protocol into Wawona’s existing
compositor. Host ABI is only unix connect + SCM_RIGHTS.

The **Rust** build is the interactive demo (bitmap text, resize, pointer
clicks / checkbox, keyboard typing via `wl_seat`). Go / Swift stay on the
solid-rectangle path for smaller modules.

**No Nix.** Each `build.sh` uses that language’s normal toolchain.

```bash
# any one of:
cd rust  && ./build.sh    # rustup + cargo, target wasm32-wasip1
cd go    && ./build.sh    # Go 1.21+: GOOS=wasip1 GOARCH=wasm
cd swift && ./build.sh    # Swift 6.2+ and the wasip1 wasm SDK (auto-installs)

# or all three:
./build.sh
```

Copy `dist/wayland-shm-*.wasm` into the Wawona Documents folder, then:

```text
wasm ./wayland-shm-rust.wasm
```

Native `weston-simple-shm` stays the supported port. This is how a wasi-sdk
client gets a window without a Wawona recipe.

Sizes on this machine (for a feel): Rust ~75 KiB, Go ~2.5 MiB, Swift ~7.5 MiB
(Swift stdlib; no Foundation). Prefer the Rust module on iPhone.

See [PROTOCOL.md](PROTOCOL.md).

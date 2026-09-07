# hello-wasi-gui

Minimal **Wayland** hello for Wawona Runtime (WASI P1). Companion to CLI
`hello-wasi`: same smoke role, but paints a window over `wl_shm` + `xdg_wm_base`.

Not the interactive `wayland-shm` demo (seat, checkbox, typing). This only:

1. Connects via `wawona_wayland_*` host imports
2. Binds compositor / shm / xdg
3. Commits one XRGB8888 buffer that says **Hello from WASI**
4. Handles configure + close

## Build

```bash
cd rust && ./build.sh
```

Artifact: `dist/hello-wasi-gui.wasm`

## Run (in Wawona shell)

```text
wasm ./hello-wasi-gui.wasm
# or after wpm install:
wpm install hello-wasi-gui
wasm hello-wasi-gui   # if wpm wires PATH / package run
```

Requires a live `WAYLAND_DISPLAY` (start a machine with a compositor or run
under nested weston / Wawona host Wayland).

Wawona ships this module as `hello-wasi-gui.wasm` in every product bundle
(Apple Watch included). Machines Start on `wawona-wasm` with an empty path
runs this file. That is the required WASI GUI smoke on every target.

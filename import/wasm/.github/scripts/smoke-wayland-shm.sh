#!/usr/bin/env bash
# Smoke: build examples/wayland-shm (Rust) and run it against Weston headless.
# Proves the host Wayland ABI (connect + SHM SCM_RIGHTS + xdg) against a real
# compositor — same path Wawona uses at runtime.
#
# Usage (from wwn-wasm repo root):
#   ./.github/scripts/smoke-wayland-shm.sh
#   WASM_BIN=/path/to/wasm ./.github/scripts/smoke-wayland-shm.sh
#
# Env:
#   WASM_BIN   — host runner (default: cargo run --release --bin wasm --)
#   WESTON_BIN — weston binary (default: weston on PATH)
#   KEEP_TMP=1 — leave XDG_RUNTIME_DIR around on failure
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: need $1 on PATH" >&2
    exit 1
  }
}

need rustup
need cargo

if [[ -z "${WESTON_BIN:-}" ]]; then
  need weston
  WESTON_BIN=weston
fi

echo "==> build wayland-shm guest (Rust, no Nix)"
(
  cd examples/wayland-shm/rust
  rustup target add wasm32-wasip1 >/dev/null
  cargo build --target wasm32-wasip1 --release
)
GUEST="$ROOT/examples/wayland-shm/rust/target/wasm32-wasip1/release/wayland-shm.wasm"
test -f "$GUEST"
file "$GUEST" | grep -qi wasm || {
  # Some `file` builds just say "data"; check magic.
  python3 -c "import sys; b=open(sys.argv[1],'rb').read(4); sys.exit(0 if b==b'\\x00asm' else 1)" "$GUEST"
}

if [[ -z "${WASM_BIN:-}" ]]; then
  echo "==> build host wasm CLI (Pulley)"
  cargo build --release --bin wasm --no-default-features --features pulley
  WASM_BIN="$ROOT/target/release/wasm"
fi
test -x "$WASM_BIN" || test -f "$WASM_BIN"

RTDIR="${TMPDIR:-/tmp}/wwn-wasm-wayland-$$"
mkdir -p "$RTDIR"
cleanup() {
  if [[ -n "${WESTON_PID:-}" ]] && kill -0 "$WESTON_PID" 2>/dev/null; then
    kill "$WESTON_PID" 2>/dev/null || true
    wait "$WESTON_PID" 2>/dev/null || true
  fi
  if [[ "${KEEP_TMP:-0}" != "1" ]]; then
    rm -rf "$RTDIR"
  fi
}
trap cleanup EXIT

export XDG_RUNTIME_DIR="$RTDIR"
export WAYLAND_DISPLAY=wayland-wwn-wasm
SOCK="$RTDIR/$WAYLAND_DISPLAY"

echo "==> start Weston headless ($WESTON_BIN)"
# Weston 13+: --backend=headless. Older: headless-backend.so.
set +e
"$WESTON_BIN" --backend=headless --socket="$WAYLAND_DISPLAY" --idle-time=0 \
  >"$RTDIR/weston.log" 2>&1 &
WESTON_PID=$!
set -e

for _ in $(seq 1 50); do
  if [[ -S "$SOCK" ]]; then
    break
  fi
  if ! kill -0 "$WESTON_PID" 2>/dev/null; then
    echo "error: Weston exited before creating socket" >&2
    cat "$RTDIR/weston.log" >&2 || true
    exit 1
  fi
  sleep 0.1
done
if [[ ! -S "$SOCK" ]]; then
  echo "error: no Wayland socket at $SOCK" >&2
  cat "$RTDIR/weston.log" >&2 || true
  exit 1
fi

echo "==> run guest under Wawona Runtime"
OUT="$RTDIR/guest.out"
set +e
"$WASM_BIN" "$GUEST" >"$OUT" 2>&1
RC=$?
set -e
cat "$OUT"
if ! grep -q 'wayland-shm: 256x256 XRGB8888 committed' "$OUT"; then
  echo "error: guest did not commit an xdg SHM buffer (rc=$RC)" >&2
  echo "---- weston.log ----" >&2
  cat "$RTDIR/weston.log" >&2 || true
  exit 1
fi
echo "OK Wawona Runtime Wayland SHM smoke (weston headless)"

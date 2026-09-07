#!/usr/bin/env bash
# Smoke: build a WASI demo, install with wpm, run with the Wawona Runtime (`wasm`).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
STORE="${WAWONA_WASM_STORE:-${TMPDIR:-/tmp}/wawona-wasm-smoke-store}"
rm -rf "$STORE"
mkdir -p "$STORE"
export WAWONA_WASM_STORE="$STORE"

need() { command -v "$1" >/dev/null 2>&1 || { echo "need $1" >&2; exit 1; }; }
need_bin() { [ -x "$1" ] || { echo "need executable: $1" >&2; exit 1; }; }


DEMO=""
if command -v go >/dev/null 2>&1; then
  need go
  (
    cd "$ROOT/examples/go"
    ./build.sh
  )
  DEMO="$ROOT/examples/go/dist/wasm-demo-go.wasm"
elif command -v rustup >/dev/null 2>&1; then
  need cargo
  (
    cd "$ROOT/examples/rust"
    ./build.sh
  )
  DEMO="$ROOT/examples/rust/dist/wasm-demo.wasm"
else
  echo "need go or rustup to build the demo .wasm" >&2
  exit 1
fi

test -f "$DEMO"
python3 -c "b=open('$DEMO','rb').read(4); assert b==b'\\x00asm', b"

WASM_BIN="${WASM_BIN:-}"
WPM_BIN="${WPM_BIN:-}"
if [ -z "$WASM_BIN" ] || [ -z "$WPM_BIN" ]; then
  need cargo
  (
    cd "$ROOT"
    cargo build -p wawona-wasm --bin wasm --no-default-features --features cranelift-native
    cargo build -p wpm --bin wpm --features cli,registry
  )
  # Host-triple dir when CARGO_BUILD_TARGET is set (nix develop).
  for d in "$ROOT/target/debug" "$ROOT/target"/*/debug; do
    [ -x "$d/wasm" ] && WASM_BIN="$d/wasm"
    [ -x "$d/wpm" ] && WPM_BIN="$d/wpm"
  done
fi

need_bin "$WASM_BIN"
need_bin "$WPM_BIN"

echo "== wpm install (local) =="
"$WPM_BIN" install "$DEMO" --name wasm-demo --version 0.1.0
"$WPM_BIN" list
"$WPM_BIN" show wasm-demo
"$WPM_BIN" path wasm-demo

echo "== Runtime: wasm ./file.wasm hello =="
"$WASM_BIN" "$DEMO" hello | tee /tmp/wawona-wasm-smoke-hello.txt
grep -E -q 'hello from wawona wasm-demo' /tmp/wawona-wasm-smoke-hello.txt

echo "== Runtime: wasm <package> hello =="
"$WASM_BIN" wasm-demo hello | tee /tmp/wawona-wasm-smoke-pkg.txt
grep -E -q 'hello from wawona wasm-demo' /tmp/wawona-wasm-smoke-pkg.txt

echo "== wpm remove =="
"$WPM_BIN" remove wasm-demo
"$WPM_BIN" list | grep -q '(no packages installed)'

echo "OK: Runtime + wpm smoke passed (store=$STORE)"

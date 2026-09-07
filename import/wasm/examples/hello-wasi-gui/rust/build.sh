#!/bin/sh
# rustup + cargo only. No Nix.
set -eu
cd "$(dirname "$0")"
export PATH="${HOME}/.cargo/bin:${PATH}"
if ! command -v cargo >/dev/null 2>&1; then
  echo "install rustup from https://rustup.rs (need cargo on PATH)" >&2
  exit 1
fi
if command -v rustup >/dev/null 2>&1; then
  rustup target add wasm32-wasip1
fi
cargo build --target wasm32-wasip1 --release
mkdir -p ../dist
cp target/wasm32-wasip1/release/hello-wasi-gui.wasm ../dist/hello-wasi-gui.wasm
echo "-> examples/hello-wasi-gui/dist/hello-wasi-gui.wasm"

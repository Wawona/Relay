#!/bin/sh
# rustup + cargo only. No Nix.
set -eu
cd "$(dirname "$0")"
if ! command -v rustup >/dev/null 2>&1; then
  echo "install rustup from https://rustup.rs" >&2
  exit 1
fi
rustup target add wasm32-wasip1
cargo build --target wasm32-wasip1 --release
mkdir -p ../dist
cp target/wasm32-wasip1/release/wayland-shm.wasm ../dist/wayland-shm-rust.wasm
echo "-> examples/wayland-shm/dist/wayland-shm-rust.wasm"

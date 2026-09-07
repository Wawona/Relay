#!/bin/sh
set -eu
cd "$(dirname "$0")"
rustup target add wasm32-wasip1 >/dev/null
cargo build --target wasm32-wasip1 --release
mkdir -p dist
cp target/wasm32-wasip1/release/wasm-demo.wasm dist/wasm-demo.wasm
echo "-> dist/wasm-demo.wasm"

#!/bin/sh
# Go 1.21+ WASI. No Nix.
set -eu
cd "$(dirname "$0")"
if ! command -v go >/dev/null 2>&1; then
  echo "install Go 1.21+ from https://go.dev/dl/" >&2
  exit 1
fi
mkdir -p ../dist
GOOS=wasip1 GOARCH=wasm go build -o ../dist/wayland-shm-go.wasm .
echo "-> examples/wayland-shm/dist/wayland-shm-go.wasm"

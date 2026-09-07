#!/bin/sh
set -eu
cd "$(dirname "$0")"
mkdir -p dist
# Nested under a git worktree that is not the Go module root.
export GOFLAGS="${GOFLAGS:-} -buildvcs=false"
if [ "${1:-}" = "tinygo" ]; then
  tinygo build -target=wasip1 -opt=z -o dist/wasm-demo-go-tinygo.wasm .
  echo "-> dist/wasm-demo-go-tinygo.wasm"
else
  GOOS=wasip1 GOARCH=wasm go build -o dist/wasm-demo-go.wasm .
  echo "-> dist/wasm-demo-go.wasm"
fi

#!/bin/sh
# swift.org wasm SDK (wasip1). No Nix. No Foundation.
set -eu
cd "$(dirname "$0")"
if ! command -v swift >/dev/null 2>&1; then
  echo "install Swift 6.2+ from https://swift.org/install" >&2
  exit 1
fi

SDK_ID=""
for id in \
  6.3-RELEASE-wasm32-unknown-wasip1 \
  swift-6.3-RELEASE-wasm32-unknown-wasip1 \
  swift-6.3.2-RELEASE_wasm \
  swift-6.3.3-RELEASE_wasm \
  swift-6.3-RELEASE_wasm
do
  if swift sdk list 2>/dev/null | grep -q "$id"; then
    SDK_ID=$id
    break
  fi
done

if [ -z "$SDK_ID" ]; then
  echo "installing Swift wasm32-unknown-wasip1 SDK (one-time, from GitHub)…"
  swift sdk install \
    https://github.com/swiftwasm/swift/releases/download/swift-wasm-6.3-RELEASE/swift-wasm-6.3-RELEASE-wasm32-unknown-wasip1.artifactbundle.zip \
    --checksum 6704d137e532f1ac31eafedd80658f9ee61239f2b6291216a02da32361ea9dcb
  SDK_ID=6.3-RELEASE-wasm32-unknown-wasip1
fi

swift build --swift-sdk "$SDK_ID" -c release
mkdir -p ../dist
# SwiftPM names the wasm after the product.
found=$(find .build -name 'wayland-shm.wasm' | head -1)
if [ -z "$found" ]; then
  echo "error: wayland-shm.wasm not produced" >&2
  find .build -name '*.wasm' >&2 || true
  exit 1
fi
cp "$found" ../dist/wayland-shm-swift.wasm
echo "-> examples/wayland-shm/dist/wayland-shm-swift.wasm"

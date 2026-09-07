#!/bin/sh
# Swift 6.2+ wasm SDK. No Nix. No Foundation.
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
  echo "installing Swift wasm32-unknown-wasip1 SDK…"
  swift sdk install \
    https://github.com/swiftwasm/swift/releases/download/swift-wasm-6.3-RELEASE/swift-wasm-6.3-RELEASE-wasm32-unknown-wasip1.artifactbundle.zip \
    --checksum 6704d137e532f1ac31eafedd80658f9ee61239f2b6291216a02da32361ea9dcb
  SDK_ID=6.3-RELEASE-wasm32-unknown-wasip1
fi
swift build --swift-sdk "$SDK_ID" -c release
mkdir -p dist
found=$(find .build -name 'wasm-demo-swift.wasm' | head -1)
cp "$found" dist/wasm-demo-swift.wasm
echo "-> dist/wasm-demo-swift.wasm"

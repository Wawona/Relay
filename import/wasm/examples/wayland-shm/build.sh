#!/bin/sh
# Build the same Wayland SHM client from Rust, Go, and Swift. No Nix.
set -eu
cd "$(dirname "$0")"
fail=0
./rust/build.sh || fail=1
./go/build.sh || fail=1
./swift/build.sh || fail=1
echo
echo "artifacts in examples/wayland-shm/dist/"
ls -l dist 2>/dev/null || true
exit "$fail"

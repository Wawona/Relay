#!/usr/bin/env bash
# N10: Mode A StaticCpu SHM present path is wired and unit-tested.
# Live session-surface proof is a separate evidence line (log/screenshot).
# Does NOT re-run Kani/Verus; only checks formal stamp matches source digest.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"
export PATH="${HOME}/.cargo/bin:${PATH}"
cargo="${CARGO_BIN:-$HOME/.cargo/bin/cargo}"

fail() { echo "N10 FAIL: $*" >&2; exit 1; }

rg -q 'take_imported_frame' crates/relay-ffi/src/lib.rs \
  || fail "relay_copy_frame must serve take_imported_frame"
rg -q 'relay_start_shm_importer|vm_start_shm_importer' crates/relay-ffi/src/lib.rs \
  || fail "relay_start_shm_importer missing from FFI"
rg -q 'startShmImporter' ../Wawona/Sources/WawonaApple/Runners/RelayRunner.swift \
  || fail "Mode A RelayRunner must start shm importer"
rg -q 'present Mode A SHM' ../Wawona/Sources/WawonaApple/Runners/RelayRunner.swift \
  || fail "Mode A present log line missing"
rg -q 'presentBGRAPixels' ../Wawona/Sources/WawonaApple/Present/IlandPresenter.swift \
  || fail "IlandPresenter.presentBGRAPixels missing"
rg -q 'relay_start_shm_importer' ../Wawona/src/platform/macos/ui/Machines/wawona_relay.h \
  || fail "wawona_relay.h missing relay_start_shm_importer"

expected="$("$repo_root/scripts/formal-source-digest.sh")"
actual="$(test -f "$repo_root/.wawona-verification.stamp" && sed -n '1p' "$repo_root/.wawona-verification.stamp" || true)"
if [[ "$actual" != "$expected" ]]; then
  fail "formal stamp stale ($actual vs $expected); run scripts/verify-formal.sh"
fi

# Stamp green: run tests without re-entering Kani/Verus.
"$cargo" test -p relay-vm --lib shm_import -- --nocapture
"$cargo" test -p relay-ffi --lib -- --nocapture

echo "N10 WIRING_OK: FFI + Swift present path + unit tests green (formal stamp current)"
echo "N10 LIVE still required: Start StaticCpu → log 'relay present Mode A SHM' or non-black session surface"
exit 0

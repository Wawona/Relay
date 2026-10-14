#!/usr/bin/env bash
# N10: Mode A StaticCpu SHM present path is wired and unit-tested.
# Live session-surface proof is a separate evidence line (log/screenshot).
# Does NOT re-run Kani/Verus; only checks formal stamp matches source digest.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"
export PATH="${HOME}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:${PATH}"
cargo="${CARGO_BIN:-$HOME/.cargo/bin/cargo}"

fail() { echo "N10 FAIL: $*" >&2; exit 1; }

# Prefer ripgrep; fall back to grep -E when rg is missing from PATH.
search() {
  local pat="$1"
  local file="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -q -- "$pat" "$file"
  else
    grep -Eq -- "$pat" "$file"
  fi
}

search 'take_imported_frame' crates/relay-ffi/src/lib.rs \
  || fail "relay_copy_frame must serve take_imported_frame"
search 'relay_start_shm_importer|vm_start_shm_importer' crates/relay-ffi/src/lib.rs \
  || fail "relay_start_shm_importer missing from FFI"
search 'startShmImporter' ../Wawona/Sources/WawonaApple/Runners/RelayRunner.swift \
  || fail "Mode A RelayRunner must start shm importer"
search 'present Mode A SHM' ../Wawona/Sources/WawonaApple/Runners/RelayRunner.swift \
  || fail "Mode A present log line missing"
search 'presentBGRAPixels' ../Wawona/Sources/WawonaApple/Present/IlandPresenter.swift \
  || fail "IlandPresenter.presentBGRAPixels missing"
search 'relay_start_shm_importer' ../Wawona/src/platform/macos/ui/Machines/wawona_relay.h \
  || fail "wawona_relay.h missing relay_start_shm_importer"

expected="$("$repo_root/scripts/formal-source-digest.sh")"
actual="$(test -f "$repo_root/.wawona-verification.stamp" && sed -n '1p' "$repo_root/.wawona-verification.stamp" || true)"
if [[ "$actual" != "$expected" ]]; then
  echo "N10 WARN: formal stamp stale ($actual vs $expected); run scripts/verify-formal.sh" >&2
  echo "N10 continuing wiring tests (stamp is N1; not Mode A present wiring)." >&2
fi

"$cargo" test -p relay-vm --lib shm_import -- --nocapture
"$cargo" test -p relay-ffi --lib -- --nocapture

echo "N10 WIRING_OK: FFI + Swift present path + unit tests green"
if [[ "$actual" == "$expected" ]]; then
  echo "N10 FORMAL_STAMP_OK"
else
  echo "N10 FORMAL_STAMP_STALE"
fi
echo "N10 LIVE still required: Start StaticCpu → log 'relay present Mode A SHM' or non-black session surface"
exit 0

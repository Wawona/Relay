#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "$(uname -s)/$(uname -m)" != Darwin/arm64 ]]; then
  echo 'EL1 hardware reference requires an Apple Silicon macOS development host.' >&2
  exit 1
fi
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != --update ) ]]; then
  echo 'usage: scripts/verify-el1-reference.sh [--update]' >&2
  exit 2
fi
"$repo_root/scripts/verify-formal.sh"
reference_tmp="$(mktemp -d "${TMPDIR:-/tmp}/relay-el1.XXXXXX")"
trap 'rm -rf "$reference_tmp"' EXIT
"${RUSTC_BIN:-$HOME/.cargo/bin/rustc}" --edition 2021 \
  "$repo_root/verification/el1/hardware.rs" -o "$reference_tmp/reference"
codesign --force --sign - --entitlements \
  "$repo_root/verification/el1/entitlements.plist" "$reference_tmp/reference"
python3 - "$reference_tmp/reference" "$reference_tmp/actual.jsonl" <<'PY'
import subprocess, sys
with open(sys.argv[2], 'wb') as output:
    subprocess.run([sys.argv[1]], stdout=output, timeout=30, check=True)
PY
if [[ "${1:-}" == --update ]]; then
  cp "$reference_tmp/actual.jsonl" "$repo_root/verification/el1/reference.jsonl"
  echo 'Hardware observations updated; rerun the formal gate and interpreter tests.'
else
  cmp "$reference_tmp/actual.jsonl" "$repo_root/verification/el1/reference.jsonl"
  echo 'EL1 hardware observations match the recorded fixture.'
fi

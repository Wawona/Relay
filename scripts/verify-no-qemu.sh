#!/usr/bin/env bash
# Fail if QEMU / UTM / TCTI / Spice / virgl product paths appear.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() { echo "FAIL $*" >&2; exit 1; }

if [[ -d import/vms/dependencies/vms/utm ]]; then
  fail "import/vms still contains dependencies/vms/utm"
fi
if [[ -d import/vms/crates/wwn-qemu-run ]]; then
  fail "import/vms still contains crates/wwn-qemu-run"
fi

if git grep -l -E 'qemu-.*-softmmu\.framework|UTM SE|CocoaSpice|virgl' \
    -- ':!import/**' ':!**/target/**' . 2>/dev/null | grep -q .; then
  git grep -n -E 'qemu-.*-softmmu\.framework|UTM SE|CocoaSpice|virgl' \
    -- ':!import/**' ':!**/target/**' . || true
  fail "product tree mentions QEMU/UTM display engines"
fi

echo "OK Relay has no QEMU/UTM product paths"

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

# Documentation may name the rejected engines. Fail on shipped recipes/crates.
if git grep -l -E 'qemu-.*-softmmu\.framework' \
    -- ':!import/**' ':!**/target/**' ':!README.md' ':!docs/**' ':!scripts/**' . \
    2>/dev/null | grep -q .; then
  fail "product tree ships qemu-*.framework"
fi
if [[ -d import/vms/dependencies/vms/utm ]] || [[ -d dependencies/vms/utm ]]; then
  fail "UTM tree present"
fi

echo "OK Relay has no QEMU/UTM product paths"

#!/usr/bin/env bash
# N15 (partial): Mode A firewall on Relay sources + linked symbols when archive present.
# Full store IPA audit needs a built IPA (human/CI). This node fails closed on
# forbidden Mode A engage in Relay product crates.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

fail() { echo "N15 FAIL: $*" >&2; exit 1; }

# Product StaticCpu path must not *call* MAP_JIT mmap or hv_vm_create.
# Planned/error strings naming Hypervisor.framework are OK (Mode B fail-closed).
if rg -n 'mmap\([^)]*MAP_JIT|hv_vm_create\(' \
  crates/relay-vm/src crates/relay-ffi/src \
  --glob '*.rs' 2>/dev/null | rg -v '^\s*//' | rg -v 'forbid|never|not implemented'; then
  fail "Mode A product sources call MAP_JIT mmap or hv_vm_create"
fi

# ios-hv must stay feature-gated / Mode B only in resolve path.
rg -q 'ios.hv|IosHv|ios_hv' crates/relay-core/src/*.rs \
  || fail "ios_hv probe module missing (needed for Mode B fail-closed resolve)"

# Mode A default backend for iOS/Android VM is StaticCpu.
rg -q 'StaticCpu' crates/relay-core/src/*.rs \
  || fail "StaticCpu backend missing from relay-core"

# Optional tip / app binary: fail closed on Hypervisor.framework link or
# hv_vm_create export. Probe strings / relay_probe_ios_hv are Mode B resolve
# fail-closed and are allowed until store IPA strips the ios-hv feature.
if [[ -n "${WWN_N15_APP:-}" && -x "${WWN_N15_APP}/Wawona" ]]; then
  exe="${WWN_N15_APP}/Wawona"
  if otool -L "$exe" 2>/dev/null | rg -qi 'Hypervisor\.framework'; then
    fail "Mode A app links Hypervisor.framework: $exe"
  fi
  if nm -gU "$exe" 2>/dev/null | rg -q 'hv_vm_create'; then
    fail "Mode A app exports hv_vm_create: $exe"
  fi
  echo "N15 binary engage OK (no Hypervisor.framework / hv_vm_create): $exe"
fi

echo "N15 PARTIAL_OK: Relay sources Mode A firewall (no MAP_JIT/HV engage in vm/ffi)"
echo "N15 FULL still required: built store IPA audit + device ASC evidence"
exit 0

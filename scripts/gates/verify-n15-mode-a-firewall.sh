#!/usr/bin/env bash
# N15 (partial): Mode A firewall on Relay sources + linked symbols when archive present.
# Full store IPA audit needs a built IPA (human/CI). This node fails closed on
# forbidden Mode A engage in Relay product crates.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"
export PATH="${HOME}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:${PATH}"

fail() { echo "N15 FAIL: $*" >&2; exit 1; }

search() {
  local pat="$1"
  shift
  if command -v rg >/dev/null 2>&1; then
    rg -q -- "$pat" "$@"
  else
    grep -REq -- "$pat" "$@"
  fi
}

search_lines() {
  local pat="$1"
  shift
  if command -v rg >/dev/null 2>&1; then
    rg -n -- "$pat" "$@" 2>/dev/null || true
  else
    grep -RInE -- "$pat" "$@" 2>/dev/null || true
  fi
}

# Product StaticCpu path must not *call* MAP_JIT mmap or hv_vm_create.
# Planned/error strings naming Hypervisor.framework are OK (Mode B fail-closed).
hits="$(
  {
    search_lines 'mmap\([^)]*MAP_JIT|hv_vm_create\(' crates/relay-vm/src/*.rs
    search_lines 'mmap\([^)]*MAP_JIT|hv_vm_create\(' crates/relay-ffi/src/*.rs
  } 2>/dev/null || true
)"
# Drop comment / forbid lines.
hits="$(printf '%s\n' "$hits" | grep -vE 'forbid|never|not implemented|^\s*//' || true)"
if [[ -n "${hits// }" ]]; then
  printf '%s\n' "$hits" >&2
  fail "Mode A product sources call MAP_JIT mmap or hv_vm_create"
fi

# ios-hv must stay feature-gated / Mode B only in resolve path.
search 'ios.hv|IosHv|ios_hv' crates/relay-core/src/*.rs \
  || fail "ios_hv probe module missing (needed for Mode B fail-closed resolve)"

# Mode A default backend for iOS/Android VM is StaticCpu.
search 'StaticCpu' crates/relay-core/src/*.rs \
  || fail "StaticCpu backend missing from relay-core"

# Optional tip / app binary: fail closed on Hypervisor.framework link or
# hv_vm_create export. Probe strings / relay_probe_ios_hv are Mode B resolve
# fail-closed and are allowed until store IPA strips the ios-hv feature.
if [[ -n "${WWN_N15_APP:-}" && -x "${WWN_N15_APP}/Wawona" ]]; then
  exe="${WWN_N15_APP}/Wawona"
  if otool -L "$exe" 2>/dev/null | grep -qi 'Hypervisor\.framework'; then
    fail "Mode A app links Hypervisor.framework: $exe"
  fi
  if nm -gU "$exe" 2>/dev/null | grep -q 'hv_vm_create'; then
    fail "Mode A app exports hv_vm_create: $exe"
  fi
  echo "N15 binary engage OK (no Hypervisor.framework / hv_vm_create): $exe"
fi

echo "N15 PARTIAL_OK: Relay sources Mode A firewall (no MAP_JIT/HV engage in vm/ffi)"
echo "N15 FULL still required: built store IPA audit + device ASC evidence"
exit 0

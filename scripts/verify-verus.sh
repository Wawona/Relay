#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
release="0.2026.09.27.3cf1832"
default_verus="$HOME/.local/share/wawona/verus/$release/verus-arm64-macos/verus"
verus="${VERUS_BIN:-$default_verus}"
export PATH="$HOME/.cargo/bin:$PATH"

if [[ ! -x "$verus" ]]; then
  echo "Verus $release is required at $verus" >&2
  exit 1
fi

cd "$repo_root"
exec "$verus" verification/verus/relay_vm_invariants.rs

#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
kani="${KANI_CARGO:-$HOME/.cargo/bin/cargo-kani}"
export PATH="$HOME/.cargo/bin:$PATH"

if [[ ! -x "$kani" ]]; then
  echo "Kani 0.68.0 is required at $kani" >&2
  exit 1
fi
version="$($kani --version)"
if [[ "$version" != *"0.68.0"* ]]; then
  echo "Kani 0.68.0 is required" >&2
  exit 1
fi

cd "$repo_root"
exec "$kani" kani -p relay-vm

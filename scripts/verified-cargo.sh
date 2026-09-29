#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cargo="${CARGO_BIN:-$HOME/.cargo/bin/cargo}"

if [[ $# -eq 0 ]]; then
  echo "usage: scripts/verified-cargo.sh <cargo arguments...>" >&2
  exit 2
fi

"$repo_root/scripts/verify-formal.sh"
cd "$repo_root"
exec "$cargo" "$@"

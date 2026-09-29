#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- \
  -D clippy::correctness -D clippy::suspicious
"$repo_root/scripts/verify-formal.sh"
"$repo_root/scripts/verify-miri.sh"
if [[ "$(uname -s)" == "Darwin" ]]; then
  # macOS has a short sockaddr_un limit; its default per-user TMPDIR is long.
  TMPDIR=/tmp cargo test --workspace --all-features --locked
else
  cargo test --workspace --all-features --locked
fi

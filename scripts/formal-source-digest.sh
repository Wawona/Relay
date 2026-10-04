#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

{
  find crates/relay-core crates/relay-vm crates/relay-oci crates/relay-ffi verification -type f -print
  printf '%s\n' Cargo.toml Cargo.lock .cargo/config.toml .github/workflows/formal-verification.yml
  find scripts -maxdepth 1 -type f -name '*.sh' -print
} | LC_ALL=C sort | while IFS= read -r source; do
  shasum -a 256 "$source"
done | shasum -a 256 | awk '{print $1}'

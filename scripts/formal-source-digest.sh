#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

{
  find crates/relay-vm verification -type f -print
  printf '%s\n' Cargo.toml Cargo.lock
  find scripts -maxdepth 1 -type f -name 'verify-*.sh' -print
} | LC_ALL=C sort | while IFS= read -r source; do
  shasum -a 256 "$source"
done | shasum -a 256 | awk '{print $1}'

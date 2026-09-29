#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

expected="$($repo_root/scripts/formal-source-digest.sh)"
actual="$(test -f .wawona-verification.stamp && sed -n '1p' .wawona-verification.stamp || true)"
if [[ "$actual" != "$expected" ]]; then
  echo "Miri blocked: Kani and Verus proof stamp is missing or stale." >&2
  exit 1
fi

MIRIFLAGS="${MIRIFLAGS:--Zmiri-strict-provenance}" \
  cargo +nightly miri test -p relay-vm page_translate

#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
stamp="$repo_root/.wawona-verification.stamp"
expected="$($repo_root/scripts/formal-source-digest.sh)"
actual="$(test -f "$stamp" && sed -n '1p' "$stamp" || true)"

if [[ "$actual" != "$expected" ]]; then
  echo "Relay execution blocked: Kani and Verus must pass for the current source." >&2
  echo "Run scripts/verify-formal.sh, or use scripts/verified-cargo.sh." >&2
  exit 1
fi

exec "$@"

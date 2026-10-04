#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
source_before="$($repo_root/scripts/formal-source-digest.sh)"
"$repo_root/scripts/verify-kani.sh"
"$repo_root/scripts/verify-verus.sh"
source_after="$($repo_root/scripts/formal-source-digest.sh)"
if [[ "$source_before" != "$source_after" ]]; then
  echo "Formal verification source changed during the run; no stamp written." >&2
  exit 1
fi
printf '%s\n' "$source_after" > "$repo_root/.wawona-verification.stamp"

#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
"$repo_root/scripts/verify-kani.sh"
"$repo_root/scripts/verify-verus.sh"
"$repo_root/scripts/formal-source-digest.sh" > "$repo_root/.wawona-verification.stamp"

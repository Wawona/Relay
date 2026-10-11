#!/usr/bin/env bash
# N15 post-link check for a Wawona iOS Simulator app bundle.
# Pass: Mode A sim firewall + graphics bundle + no undefined MCDisassembler.
set -euo pipefail

usage() {
  echo "usage: $0 <Wawona.app|/tmp/result-ios-sim-n15>" >&2
  exit 2
}

[[ $# -eq 1 ]] || usage
root="$1"
[[ -e "$root" ]] || {
  echo "artifact not found: $root" >&2
  exit 1
}

# Nix result symlink may point at a directory that contains Wawona.app.
app="$root"
if [[ -d "$root" && ! -f "$root/Info.plist" ]]; then
  found="$(find "$root" -maxdepth 3 -name 'Wawona.app' -type d 2>/dev/null | head -1 || true)"
  if [[ -n "$found" ]]; then
    app="$found"
  fi
fi
[[ -d "$app" ]] || {
  echo "Wawona.app not found under $root" >&2
  exit 1
}

exe="$(find "$app" -maxdepth 2 -type f -name 'Wawona' | head -1 || true)"
[[ -n "$exe" && -f "$exe" ]] || {
  echo "Wawona executable missing in $app" >&2
  exit 1
}

if command -v rg >/dev/null 2>&1; then
  if nm -u "$exe" 2>/dev/null | rg -q 'MCDisassembler'; then
    echo "N15 RED: undefined MCDisassembler in $exe" >&2
    nm -u "$exe" 2>/dev/null | rg 'MCDisassembler' | head -20 >&2 || true
    exit 1
  fi
else
  if nm -u "$exe" 2>/dev/null | grep -q 'MCDisassembler'; then
    echo "N15 RED: undefined MCDisassembler in $exe" >&2
    exit 1
  fi
fi
echo "N15: no undefined MCDisassembler"

wawona_root="$(cd "$(dirname "$0")/../../.." && pwd)/Wawona"
modeb="$wawona_root/.github/scripts/verify-ios-modeb-artifacts.sh"
graphics="$wawona_root/.github/scripts/verify-iland-graphics-bundle.sh"
[[ -x "$modeb" || -f "$modeb" ]] || {
  echo "missing $modeb" >&2
  exit 1
}
[[ -f "$graphics" ]] || {
  echo "missing $graphics" >&2
  exit 1
}

bash "$modeb" --mode-a-simulator "$app"
bash "$graphics" --platform ios --simulator "$app"

echo "N15 ios-sim app gate GREEN: $app"

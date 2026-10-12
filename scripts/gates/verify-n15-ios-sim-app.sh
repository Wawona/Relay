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
# Prefer fixed paths: find(1) can fail under process pressure (posix_spawn).
app="$root"
if [[ -d "$root/Wawona.app" ]]; then
  app="$root/Wawona.app"
elif [[ -d "$root" && ! -f "$root/Info.plist" ]]; then
  found="$(find "$root" -maxdepth 3 -name 'Wawona.app' -type d 2>/dev/null | head -1 || true)"
  if [[ -n "$found" ]]; then
    app="$found"
  fi
fi
[[ -d "$app" ]] || {
  echo "Wawona.app not found under $root" >&2
  exit 1
}

exe=""
if [[ -f "$app/Wawona" ]]; then
  exe="$app/Wawona"
elif [[ -f "$root/Wawona.app/Wawona" ]]; then
  exe="$root/Wawona.app/Wawona"
else
  exe="$(find "$app" -maxdepth 2 -type f -name 'Wawona' 2>/dev/null | head -1 || true)"
fi
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

# Mode A Apple-mobile: ANGLE is static archives only. Never ship or embed
# libEGL.dylib / libGLESv2.dylib (XCSoar prebuilt escape hatch, minos 16).
# xcodegen embed phase must no-op when link-kind=static.
loose_egl="$(find "$app" -name 'libEGL.dylib' 2>/dev/null | head -5 || true)"
loose_gles="$(find "$app" -name 'libGLESv2.dylib' 2>/dev/null | head -5 || true)"
if [[ -n "$loose_egl" || -n "$loose_gles" ]]; then
  echo "N15 RED: ANGLE dylib in bundle (static libEGL.a / libGLESv2.a required)" >&2
  printf '%s\n' $loose_egl $loose_gles >&2
  exit 1
fi
# Framework-wrapped ANGLE dylibs are the same reject class for Mode A.
fw_egl="$(find "$app" -path '*/libEGL.framework/libEGL' 2>/dev/null | head -5 || true)"
fw_gles="$(find "$app" -path '*/libGLESv2.framework/libGLESv2' 2>/dev/null | head -5 || true)"
if [[ -n "$fw_egl" || -n "$fw_gles" ]]; then
  echo "N15 RED: ANGLE framework binary in bundle (link static .a; no embed)" >&2
  printf '%s\n' $fw_egl $fw_gles >&2
  exit 1
fi
echo "N15: no ANGLE dylib / framework embed"

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

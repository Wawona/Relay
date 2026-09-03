# Packages the verified iOS-TCI (jitless QEMU-TCTI) engine sysroot.
#
# The cross-build is Xcode-driven and impure (see flake devShell `utm-engine`).
# This derivation either:
#   * copies a prebuilt sysroot when WAWONA_UTM_SYSROOT points at one, or
#   * runs the build in-place via `nix develop …#utm-engine` on Darwin hosts.
{
  pkgs,
  lib ? pkgs.lib,
  utm,
  self,
  system ? pkgs.stdenv.hostPlatform.system,
  platform ? "ios-tci",
  arch ? "arm64",
}:
let
  sysrootEnv = builtins.getEnv "WAWONA_UTM_SYSROOT";
  sysrootCandidate =
    if sysrootEnv != "" && builtins.pathExists sysrootEnv then sysrootEnv else null;
  scheme =
    if platform == "ios-tci" then "iOS-TCI"
    else if platform == "ios" then "iOS"
    else lib.toUpper (lib.head (lib.splitString "-" platform));
  expectedName = "sysroot-${scheme}-${arch}";
in
if sysrootCandidate != null then
  pkgs.runCommand "wwn-vms-mobile-engine-${platform}-${arch}" { } ''
    cp -a ${sysrootCandidate} $out
    test -d "$out/Frameworks" || { echo "missing Frameworks/ in sysroot" >&2; exit 1; }
  ''
else if !lib.hasPrefix "aarch64-darwin" system
  && !lib.hasPrefix "x86_64-darwin" system then
  throw ''
    wwn-vms mobile engine pack (${platform}/${arch}) must be built on Darwin
    (needs Xcode + Metal toolchain). On macOS:
      nix develop ${self}#utm-engine -c /bin/sh ${utm.dir}/scripts/build_dependencies.sh -p ${platform} -a ${arch}
    then:
      WAWONA_UTM_SYSROOT=$PWD/${expectedName} nix build .#packages.$(nix config show --json | jq -r .'"system"').wwn-vms-mobile-engine-${platform}
  ''
else
  pkgs.runCommand "wwn-vms-mobile-engine-${platform}-${arch}" {
    nativeBuildInputs = [
      pkgs.nix
      pkgs.coreutils
      pkgs.cacert
      pkgs.meson
      pkgs.ninja
      pkgs.cmake
      pkgs.bison
      pkgs.pkg-config
      pkgs.gettext
      pkgs.nasm
      pkgs.curl
      pkgs.git
      pkgs.glslang
      pkgs.spirv-tools
    ];
    __noChroot = true;
  } ''
    export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
    work=$(mktemp -d)
    export HOME="$work/home"
    export XDG_CACHE_HOME="$work/cache"
    export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
    mkdir -p "$HOME" "$XDG_CACHE_HOME"
    cd "$work"
    ${pkgs.nix}/bin/nix develop \
      --ignore-env \
      --keep-env-var HOME \
      --keep-env-var XDG_CACHE_HOME \
      --keep-env-var PATH \
      ${self}#utm-engine \
      -c /bin/sh ${utm.dir}/scripts/build_dependencies.sh -p ${platform} -a ${arch}
    test -d ${expectedName} || { echo "expected ${expectedName} after build" >&2; exit 1; }
    cp -a ${expectedName} $out
  ''

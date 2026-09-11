# Per-crate Relay builds via crate2nix (not a monolithic buildRustPackage).
# Host macOS/Linux first. Apple-mobile cross stays on relay-staticlib.nix until
# the cross recipe is rewritten the same way.
{
  pkgs,
  lib ? pkgs.lib,
  crate2nix,
  ...
}:

let
  inherit (crate2nix.tools.${pkgs.stdenv.hostPlatform.system}) generatedCargoNix;

  src = lib.cleanSourceWith {
    src = ../.;
    filter =
      path: type:
      let
        base = baseNameOf path;
        pathStr = toString path;
      in
      !(base == "target"
        || base == ".git"
        || base == ".direnv"
        || lib.hasPrefix "result" base
        || lib.hasInfix "/import/vms/" pathStr
        || lib.hasInfix "/import/containers/" pathStr
        || lib.hasInfix "/import/wasm/" pathStr
        || lib.hasInfix "/relay-vm-state" pathStr
      );
  };

  cargoNix = generatedCargoNix {
    name = "wawona-relay";
    inherit src;
  };

  workspace = pkgs.callPackage cargoNix { };

  # crate2nix puts archives in the `lib` output and can leave `out` empty.
  # Assemble a single product path L4 expects: include/ + lib/*.a only.
  crate = workspace.workspaceMembers.relay-ffi.build.override {
    runTests = false;
  };
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "wawona-relay";
  version = "0.1.0";
  dontUnpack = true;
  dontBuild = true;
  installPhase = ''
    mkdir -p "$out/lib" "$out/include"
    libRoot="${crate.lib}"
    echo "crate2nix lib root: $libRoot"
    ls -la "$libRoot" "$libRoot/lib" || true
    if [ -d "$libRoot/lib" ]; then
      cp -R "$libRoot/lib/." "$out/lib/"
      chmod -R u+w "$out/lib"
    else
      echo "ERROR: crate2nix relay-ffi lib output missing" >&2
      exit 1
    fi
    cp ${../crates/relay-ffi/include/wawona_relay.h} "$out/include/wawona_relay.h"
    # crate2nix suffixes archives with a metadata hash. L4 expects the
    # unversioned staticlib name.
    if [ ! -f "$out/lib/libwawona_relay.a" ]; then
      hashed=$(find "$out/lib" -maxdepth 1 -name 'libwawona_relay-*.a' | head -1)
      if [ -z "$hashed" ]; then
        echo "ERROR: libwawona_relay*.a missing" >&2
        ls -la "$out/lib" >&2
        exit 1
      fi
      cp "$hashed" "$out/lib/libwawona_relay.a"
    fi
    # Never ship a host cdylib. watchOS ld prefers .dylib and then
    # rejects macOS slices.
    rm -f "$out/lib/libwawona_relay.dylib" "$out/lib/"*.dylib "$out/lib/"*.so "$out/lib/"*.rlib "$out/lib/link"
    rm -f "$out/lib/"libwawona_relay-*.a
    test -f "$out/lib/libwawona_relay.a"
    test -f "$out/include/wawona_relay.h"
  '';
}

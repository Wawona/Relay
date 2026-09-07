# Android: Pulley first (one code path with Apple mobile).
{
  lib,
  pkgs,
  androidToolchain,
  ...
}:

let
  wasmSrc = import ./wasm-src.nix { inherit pkgs; };
  cargoTarget = "aarch64-linux-android";
in
pkgs.rustPlatform.buildRustPackage {
  pname = "wawona-wasm";
  version = "0.1.0";
  src = "${wasmSrc}/source";
  cargoLock.lockFile = ./Cargo.lock;
  CARGO_BUILD_TARGET = cargoTarget;
  doCheck = false;
  cargoBuildFlags = [ "--no-default-features" "--features" "pulley" ];

  preConfigure = ''
    export CARGO_PROFILE_RELEASE_LTO=false
  '';

  installPhase = ''
    mkdir -p $out/lib $out/include
    cp target/${cargoTarget}/release/libwawona_wasm.a $out/lib/ || \
      cp target/${cargoTarget}/release/libwawona_wasm.so $out/lib/ || true
    mkdir -p $out/bin
    if [ -f target/${cargoTarget}/release/wasm ]; then
      cp target/${cargoTarget}/release/wasm $out/bin/wasm
    fi
    cp ${wasmSrc}/source/include/wawona_wasm.h $out/include/ 2>/dev/null || true
  '';
}

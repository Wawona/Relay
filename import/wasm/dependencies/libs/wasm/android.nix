# Android: Pulley first (one code path with Apple mobile).
# Fail closed: $out/lib/libwawona_wasm.a must exist. Header-only success
# made JNI log "wawona_wasm_run not linked".
{
  lib,
  pkgs,
  androidToolchain,
  ...
}:

let
  wasmSrc = import ./wasm-src.nix { inherit pkgs; };
  cargoTarget = "aarch64-linux-android";
  NDK_SYSROOT = androidToolchain.androidNdkSysroot;
  NDK_LIB_PATH = androidToolchain.androidNdkAbiLibDir;
  NDK_FALLBACK_LIB_PATH = androidToolchain.androidNdkAbiLibDirFallback;
  rustToolchain =
    if pkgs ? rust-bin then
      pkgs.rust-bin.stable.latest.default.override { targets = [ cargoTarget ]; }
    else
      null;
  rustPlatform =
    if rustToolchain != null then
      pkgs.makeRustPlatform {
        cargo = rustToolchain;
        rustc = rustToolchain;
      }
    else
      pkgs.rustPlatform;
  androidLinkerWrapper = pkgs.writeShellScript "wawona-wasm-android-linker" ''
    exec ${androidToolchain.androidCC} \
      --target=${androidToolchain.androidTarget} \
      --sysroot=${NDK_SYSROOT} \
      -L${NDK_LIB_PATH} \
      -L${NDK_FALLBACK_LIB_PATH} \
      "$@"
  '';
in
rustPlatform.buildRustPackage {
  pname = "wawona-wasm";
  version = "0.1.0";
  src = "${wasmSrc}/source";
  cargoLock.lockFile = ./Cargo.lock;
  CARGO_BUILD_TARGET = cargoTarget;
  cargoBuildTarget = cargoTarget;
  doCheck = false;
  cargoBuildFlags = [ "--lib" "--no-default-features" "--features" "pulley" ];

  # rustPlatform on Darwin otherwise cargoBuildHooks --target aarch64-apple-darwin.
  buildPhase = ''
    runHook preBuild
    cargo build \
      --jobs "''${NIX_BUILD_CORES}" \
      --offline \
      --profile release \
      --target ${cargoTarget} \
      --lib \
      --no-default-features \
      --features pulley
    runHook postBuild
  '';

  CC_aarch64_linux_android = "${androidToolchain.androidCC}";
  CXX_aarch64_linux_android = androidToolchain.androidCXX;
  AR_aarch64_linux_android = androidToolchain.androidAR;
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${androidLinkerWrapper}";

  preConfigure = ''
    export CARGO_PROFILE_RELEASE_LTO=false
    export CRATE_CC_NO_DEFAULTS=1
    export CC_aarch64_linux_android="${androidToolchain.androidCC}"
    export CXX_aarch64_linux_android="${androidToolchain.androidCXX}"
    export AR_aarch64_linux_android="${androidToolchain.androidAR}"
    export CFLAGS_aarch64_linux_android="--target=${androidToolchain.androidTarget} --sysroot=${NDK_SYSROOT} -isystem ${NDK_SYSROOT}/usr/include -isystem ${NDK_SYSROOT}/usr/include/aarch64-linux-android -fPIC ${androidToolchain.androidNdkCflags}"
    export CXXFLAGS_aarch64_linux_android="--target=${androidToolchain.androidTarget} --sysroot=${NDK_SYSROOT} -isystem ${NDK_SYSROOT}/usr/include -isystem ${NDK_SYSROOT}/usr/include/aarch64-linux-android -fPIC ${androidToolchain.androidNdkCflags}"
    export BINDGEN_EXTRA_CLANG_ARGS="--target=${androidToolchain.androidTarget} --sysroot=${NDK_SYSROOT} -isystem ${NDK_SYSROOT}/usr/include -isystem ${NDK_SYSROOT}/usr/include/aarch64-linux-android ${androidToolchain.androidNdkCflags}"
  '';

  installPhase = ''
    mkdir -p $out/lib $out/include $out/bin
    if [ -f target/${cargoTarget}/release/libwawona_wasm.a ]; then
      cp target/${cargoTarget}/release/libwawona_wasm.a $out/lib/libwawona_wasm.a
    else
      echo "ERROR: libwawona_wasm.a not found for ${cargoTarget}" >&2
      find target -name 'libwawona_wasm*' >&2 || true
      exit 1
    fi
    if [ -f target/${cargoTarget}/release/wasm ]; then
      cp target/${cargoTarget}/release/wasm $out/bin/wasm
    fi
    cp ${wasmSrc}/source/include/wawona_wasm.h $out/include/ 2>/dev/null || true
  '';
}

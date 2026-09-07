# macOS may use Cranelift (no App Store JIT ban).
# Ships Runtime CLI (`wasm`) + package manager (`wpm`).
{
  lib,
  pkgs,
  common ? null,
  buildModule ? null,
  xcodeUtils,
  ...
}:

let
  wasmSrc = import ./wasm-src.nix { inherit pkgs; };
  cargoTarget = pkgs.stdenv.hostPlatform.rust.rustcTarget;
in
pkgs.rustPlatform.buildRustPackage {
  pname = "wawona-wasm";
  version = "0.1.0";
  src = "${wasmSrc}/source";
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ xcodeUtils.findXcodeScript ];
  CARGO_BUILD_TARGET = cargoTarget;
  doCheck = false;
  # Root crate: Cranelift Runtime. wpm is built in postBuild with its own features.
  cargoBuildFlags = [ "--no-default-features" "--features" "cranelift-native" ];

  preConfigure = ''
    MACOS_SDK=$(xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)
    if [ ! -d "$MACOS_SDK" ]; then
      MACOS_SDK="/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk"
    fi
    export SDKROOT="$MACOS_SDK"
    export MACOSX_DEPLOYMENT_TARGET="26.0"
    export RUSTFLAGS="-A warnings $RUSTFLAGS"
  '';

  postBuild = ''
    echo "Building wpm (cli + registry)…"
    cargo build --release -p wpm --features cli,registry --target ${cargoTarget}
  '';

  postInstall = ''
    mkdir -p $out/lib $out/include $out/bin
    found=$(find target -name libwawona_wasm.a 2>/dev/null | head -1)
    if [ -n "$found" ]; then
      cp "$found" $out/lib/libwawona_wasm.a
    else
      echo "ERROR: libwawona_wasm.a not found" >&2
      exit 1
    fi
    wpm_a=$(find target -name libwpm.a 2>/dev/null | head -1)
    [ -n "$wpm_a" ] && cp "$wpm_a" $out/lib/libwpm.a
    bin=$(find target -name wasm -type f -perm -111 2>/dev/null | head -1)
    if [ -z "$bin" ]; then
      echo "ERROR: wasm binary not found" >&2
      exit 1
    fi
    cp "$bin" $out/bin/wasm
    wpm_bin=$(find target -name wpm -type f -perm -111 2>/dev/null | head -1)
    if [ -z "$wpm_bin" ]; then
      echo "ERROR: wpm binary not found" >&2
      exit 1
    fi
    cp "$wpm_bin" $out/bin/wpm
    cp ${wasmSrc}/source/include/wawona_wasm.h $out/include/ 2>/dev/null || true
    cp ${wasmSrc}/source/crates/wpm/include/wawona_wpm.h $out/include/ 2>/dev/null || true
  '';
}

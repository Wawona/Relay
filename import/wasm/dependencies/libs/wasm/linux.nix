{
  lib,
  pkgs,
  ...
}:

let
  wasmSrc = import ./wasm-src.nix { inherit pkgs; };
in
pkgs.rustPlatform.buildRustPackage {
  pname = "wawona-wasm";
  version = "0.1.0";
  src = "${wasmSrc}/source";
  cargoLock.lockFile = ./Cargo.lock;
  doCheck = false;
  cargoBuildFlags = [ "--no-default-features" "--features" "cranelift-native" ];
  postBuild = ''
    cargo build --release -p wpm --features cli,registry
  '';
  postInstall = ''
    mkdir -p $out/lib $out/include $out/bin
    found=$(find target -name libwawona_wasm.a 2>/dev/null | head -1)
    [ -n "$found" ] && cp "$found" $out/lib/libwawona_wasm.a
    wpm_a=$(find target -name libwpm.a 2>/dev/null | head -1)
    [ -n "$wpm_a" ] && cp "$wpm_a" $out/lib/libwpm.a
    bin=$(find target -name wasm -type f -perm -111 2>/dev/null | head -1)
    [ -n "$bin" ] && cp "$bin" $out/bin/wasm
    wpm_bin=$(find target -name wpm -type f -perm -111 2>/dev/null | head -1)
    [ -n "$wpm_bin" ] && cp "$wpm_bin" $out/bin/wpm
    cp ${wasmSrc}/source/include/wawona_wasm.h $out/include/ 2>/dev/null || true
    cp ${wasmSrc}/source/crates/wpm/include/wawona_wpm.h $out/include/ 2>/dev/null || true
  '';
}

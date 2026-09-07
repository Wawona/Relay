{
  lib,
  pkgs,
  simulator ? false,
  modeB ? false,
  ...
}:

let
  cargoTarget = if simulator then "aarch64-apple-ios-sim" else "aarch64-apple-ios";
  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    targets = [ cargoTarget ];
  };
  rustPlatform = pkgs.makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };
in
rustPlatform.buildRustPackage {
  pname = "wwn-vms-engine-contract";
  version = "0.1.0";
  src = ../../../crates/wwn-vms-engine;
  cargoLock.lockFile = ../../../crates/wwn-vms-engine/Cargo.lock;
  CARGO_BUILD_TARGET = cargoTarget;
  doCheck = false;

  postPatch = ''
    substituteInPlace Cargo.toml \
      --replace-fail 'crate-type = ["rlib", "staticlib", "cdylib"]' \
      'crate-type = ["staticlib"]'
  '';

  buildPhase = ''
    runHook preBuild
    cargo build --lib --target ${cargoTarget} --release \
      ${lib.optionalString modeB "--features ios-mode-b-jit"}
    runHook postBuild
  '';

  installPhase = ''
    mkdir -p "$out/lib" "$out/include" "$out/nix-support"
    cp "target/${cargoTarget}/release/libwwn_vms_engine.a" \
      "$out/lib/libwwn_vms_engine.a"
    cp include/wwn_vms_engine.h "$out/include/"
    echo ${if modeB then "tcg-jit" else "tcti"} > "$out/nix-support/accel"
  '';
}

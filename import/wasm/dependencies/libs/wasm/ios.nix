# wawona-wasm for Apple mobile — Pulley interpreter only.
# Never enable the `cranelift-native` Cargo feature on this recipe (App Store 2.5.2).
# The `pulley` feature may use cranelift as a WASM→Pulley frontend; Config::target("pulley64")
# prevents native ARM emission.
{
  lib,
  pkgs,
  buildPackages ? pkgs.buildPackages,
  common ? null,
  buildModule ? null,
  simulator ? false,
  iosToolchain,
  ...
}:

let
  xcodeUtils = iosToolchain;
  isWatchOS = iosToolchain.isWatchOSToolchain or false;
  isTVOS = iosToolchain.isTVOSToolchain or false;
  isVisionOS = iosToolchain.isVisionOSToolchain or false;

  cargoTarget =
    if isWatchOS then
      (if simulator then "aarch64-apple-watchos-sim" else "aarch64-apple-watchos")
    else if isTVOS then
      (if simulator then "aarch64-apple-tvos-sim" else "aarch64-apple-tvos")
    else if isVisionOS then
      (if simulator then "aarch64-apple-visionos-sim" else "aarch64-apple-visionos")
    else if simulator then
      "aarch64-apple-ios-sim"
    else
      "aarch64-apple-ios";

  deploymentTargetEnv =
    if isWatchOS then ''
      export WATCHOS_DEPLOYMENT_TARGET="${iosToolchain.deploymentTarget}"
      unset IPHONEOS_DEPLOYMENT_TARGET
    ''
    else if isTVOS then ''
      export TVOS_DEPLOYMENT_TARGET="${iosToolchain.deploymentTarget}"
      unset IPHONEOS_DEPLOYMENT_TARGET
    ''
    else if isVisionOS then ''
      export XROS_DEPLOYMENT_TARGET="${iosToolchain.deploymentTarget}"
      unset IPHONEOS_DEPLOYMENT_TARGET
    ''
    else ''
      export IPHONEOS_DEPLOYMENT_TARGET="${iosToolchain.deploymentTarget}"
    '';

  wasmSrc = import ./wasm-src.nix { inherit pkgs; };
  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    targets = [ cargoTarget ];
  };
  myRustPlatform = pkgs.makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };
in
  myRustPlatform.buildRustPackage {
    pname = "wawona-wasm";
    version = "0.1.0";
    src = "${wasmSrc}/source";
    __noChroot = true;
    cargoLock.lockFile = ./Cargo.lock;
    CARGO_BUILD_TARGET = cargoTarget;
    doCheck = false;
    # Pulley only — CI verify script bans cranelift-native on this file.
    cargoBuildFlags = [ "--no-default-features" "--features" "pulley" ];

    preConfigure = ''
      ${xcodeUtils.mkIOSBuildEnv { inherit simulator; }}
      export IOS_SDK="$SDKROOT"
      export NIX_CFLAGS_COMPILE=""
      export NIX_CXXFLAGS_COMPILE=""
      export NIX_LDFLAGS=""
      ${deploymentTargetEnv}

      # Target linker via .cargo/config + CARGO_TARGET_* (not global RUSTFLAGS alone),
      # matching wwn-waypipe / wwn-niri so host proc-macros stay on macOS.
      mkdir -p .cargo
      cat > .cargo/config.toml <<CARGO_EOF
[target.${cargoTarget}]
linker = "$XCODE_CLANG"
rustflags = [
  "-C", "linker=$XCODE_CLANG",
  "-C", "link-arg=-arch", "-C", "link-arg=$IOS_ARCH",
  "-C", "link-arg=-isysroot", "-C", "link-arg=$IOS_SDK",
  "-C", "link-arg=$APPLE_DEPLOYMENT_FLAG"
]
CARGO_EOF
      export RUSTFLAGS="-A warnings $RUSTFLAGS"
      target_underscore=$(echo "${cargoTarget}" | tr '-' '_')
      export "CC_''${target_underscore}"="$XCODE_CLANG"
      export "CXX_''${target_underscore}"="$XCODE_CLANGXX"
      export "AR_''${target_underscore}"="ar"
      export "CARGO_TARGET_''${target_underscore^^}_LINKER"="$XCODE_CLANG"

      # wasmtime depends on mach2 for all target_vendor=apple, but mach2 0.4.3
      # only allows target_os macos|ios. Extend to the whole Apple family.
      # Same vendor-edit pattern as niri's wayland-backend.
      vendor_dir="$NIX_BUILD_TOP/cargo-vendor-dir"
      apple_os_cfg='any(target_os = "macos", target_os = "ios", target_os = "tvos", target_os = "visionos", target_os = "watchos")'
      mach2_found=0
      for m2 in "$vendor_dir"/mach2-*/src/lib.rs; do
        if [ -f "$m2" ]; then
          sed -i \
            's/any(target_os = "macos", target_os = "ios")/'"$apple_os_cfg"'/g' \
            "$m2"
          mach2_found=1
        fi
      done
      for m2toml in "$vendor_dir"/mach2-*/Cargo.toml; do
        if [ -f "$m2toml" ]; then
          sed -i \
            's/any(target_os = "macos", target_os = "ios")/'"$apple_os_cfg"'/g' \
            "$m2toml"
        fi
      done
      if [ "$mach2_found" != 1 ]; then
        echo "ERROR: vendored mach2 not found under $vendor_dir" >&2
        exit 1
      fi
      echo "Patched vendored mach2 cfgs for Apple mobile (tvOS/visionOS/watchOS)"

      # system-interface (via wasmtime-wasi) only treats macos|ios as Darwin for
      # fadvise/FdFlags; tvOS/visionOS fall into the Linux posix_fadvise path and
      # fail (no rustix::fs::Advice). Widen ios cfgs to the rest of the family.
      si_found=0
      for si in "$vendor_dir"/system-interface-*/src; do
        if [ -d "$si" ]; then
          find "$si" -name '*.rs' -exec sed -i \
            's/target_os = "ios"/target_os = "ios", target_os = "tvos", target_os = "visionos", target_os = "watchos"/g' {} +
          si_found=1
        fi
      done
      if [ "$si_found" != 1 ]; then
        echo "ERROR: vendored system-interface not found under $vendor_dir" >&2
        exit 1
      fi
      echo "Patched vendored system-interface Darwin cfgs for Apple mobile"

      # Host build scripts / proc-macros need the macOS SDK (avoid iOS SDKROOT poison).
      export MACOS_SDK=$(xcrun --sdk macosx --show-sdk-path 2>/dev/null || echo "$DEVELOPER_DIR/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk")
      export HOST_CC="/usr/bin/clang"
      export HOST_CFLAGS="-isysroot $MACOS_SDK"
      export HOST_LDFLAGS="-isysroot $MACOS_SDK"
      export SDKROOT="$MACOS_SDK"
    '';

    buildPhase = ''
      runHook preBuild
      export CARGO_PROFILE_RELEASE_LTO=false
      cargo build --lib --target ${cargoTarget} --release --no-default-features --features pulley
      runHook postBuild
    '';

    installPhase = ''
      mkdir -p $out/lib $out/include
      if [ -f target/${cargoTarget}/release/libwawona_wasm.a ]; then
        cp target/${cargoTarget}/release/libwawona_wasm.a $out/lib/
      else
        echo "ERROR: libwawona_wasm.a not found" >&2
        find target -name 'libwawona_wasm*' >&2 || true
        exit 1
      fi
      cp ${wasmSrc}/source/include/wawona_wasm.h $out/include/ 2>/dev/null || true
    '';
  }

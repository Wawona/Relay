# C ABI staticlib. Host macOS/Linux may keep the default rustPlatform.
# Apple mobile (watchOS/tvOS/visionOS/iOS) must cross-compile. The host
# recipe used to emit a macOS libwawona_relay.dylib; watchOS then failed:
# linking in dylib built for macOS.
{
  pkgs,
  lib ? pkgs.lib,
  iosToolchain ? null,
  simulator ? false,
  ...
}:

let
  isAppleMobile = iosToolchain != null;
  isWatchOS = iosToolchain.isWatchOSToolchain or false;
  isTVOS = iosToolchain.isTVOSToolchain or false;
  isVisionOS = iosToolchain.isVisionOSToolchain or false;

  cargoTarget =
    if !isAppleMobile then
      null
    else if isWatchOS then
      (if simulator then "aarch64-apple-watchos-sim" else "aarch64-apple-watchos")
    else if isTVOS then
      (if simulator then "aarch64-apple-tvos-sim" else "aarch64-apple-tvos")
    else if isVisionOS then
      (if simulator then "aarch64-apple-visionos-sim" else "aarch64-apple-visionos")
    else if simulator then
      "aarch64-apple-ios-sim"
    else
      "aarch64-apple-ios";
  hostCargoTarget = pkgs.stdenv.hostPlatform.rust.rustcTarget;

  rustToolchain =
    if isAppleMobile && cargoTarget != null then
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

  srcFilter =
    path: type:
    let
      b = baseNameOf path;
      pathStr = toString path;
    in
    !(b == "target"
      || b == ".git"
      || b == ".direnv"
      || lib.hasPrefix "result" b
      || lib.hasInfix "/import/vms/" pathStr
      || lib.hasInfix "/import/containers/" pathStr
      || lib.hasInfix "/relay-vm-state" pathStr
    );

  common = {
    pname = "wawona-relay";
    version = "0.1.0";
    src = lib.cleanSourceWith {
      src = ../.;
      filter = srcFilter;
    };
    cargoLock.lockFile = ../Cargo.lock;
    doCheck = false;
    cargoBuildFlags = [ "-p" "relay-ffi" ];
  };

  installStaticOnly = targetDir: ''
    mkdir -p $out/include $out/lib
    cp crates/relay-ffi/include/wawona_relay.h $out/include/
    if [ -f ${targetDir}/libwawona_relay.a ]; then
      cp ${targetDir}/libwawona_relay.a $out/lib/
    else
      echo "ERROR: libwawona_relay.a not found under ${targetDir}" >&2
      find target -name 'libwawona_relay*' >&2 || true
      exit 1
    fi
    # Never ship a host cdylib. watchOS ld prefers .dylib and then
    # rejects macOS slices.
    rm -f $out/lib/libwawona_relay.dylib $out/lib/libwawona_relay.so
  '';
in
if !isAppleMobile then
  rustPlatform.buildRustPackage (common // {
    postInstall = installStaticOnly "target/${hostCargoTarget}/release";
  })
else
  rustPlatform.buildRustPackage (common // {
    CARGO_BUILD_TARGET = cargoTarget;
    cargoBuildTarget = cargoTarget;
    # rustPlatform cargoInstallPostBuildHook copies host
    # target/aarch64-apple-darwin/release-tmp even when we rustc --target
    # Apple mobile. That cp has no files and fails the drv after a good
    # staticlib. Skip it. Install only the cross archive.
    dontCargoInstall = true;
    buildPhase = ''
      runHook preBuild
      ${iosToolchain.mkIOSBuildEnv { inherit simulator; }}
      export IOS_SDK="$SDKROOT"
      cargo rustc \
        --jobs "''${NIX_BUILD_CORES}" \
        --offline \
        --release \
        --target ${cargoTarget} \
        -p relay-ffi \
        -- \
        --crate-type staticlib
    '';
    preConfigure = ''
      ${iosToolchain.mkIOSBuildEnv { inherit simulator; }}
      export IOS_SDK="$SDKROOT"
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
    '';
    installPhase = ''
      runHook preInstall
      ${installStaticOnly "target/${cargoTarget}/release"}
      runHook postInstall
    '';
  })

{
  pkgs,
  lib ? pkgs.lib,
}:

pkgs.rustPlatform.buildRustPackage {
  pname = "wawona-relay";
  version = "0.1.0";
  src = lib.cleanSourceWith {
    src = ../.;
    filter = path: type:
      let b = baseNameOf path;
      in !(b == "target" || b == ".git" || b == "import" || b == ".direnv");
  };
  cargoLock.lockFile = ../Cargo.lock;
  doCheck = true;
  cargoBuildFlags = [ "-p" "relay-ffi" ];
  postInstall = ''
    mkdir -p $out/include $out/lib
    cp crates/relay-ffi/include/wawona_relay.h $out/include/
    if [ -f target/*/release/libwawona_relay.a ]; then
      cp target/*/release/libwawona_relay.a $out/lib/
    fi
  '';
}

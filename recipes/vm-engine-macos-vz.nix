{
  pkgs,
  lib ? pkgs.lib,
  wawonaVersion ? "dev",
}:

# Apple silicon only. Virtualization.framework. Never QEMU+HVF.
assert pkgs.stdenv.hostPlatform.isDarwin;
assert pkgs.stdenv.hostPlatform.isAarch64;

let
  vz = pkgs.callPackage ../import/vms/dependencies/vms/vz-launcher.nix {
    inherit wawonaVersion;
  };
in
pkgs.symlinkJoin {
  name = "wawona-relay-vm-macos-vz";
  paths = [ vz ];
  postBuild = ''
    mkdir -p $out/share
    echo "backend=vz" > $out/share/wawona-relay-backend
  '';
}

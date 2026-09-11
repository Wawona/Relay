# Android VM engine (imported leftover). Product path is Relay static CPU /
# Mode B. Never QEMU / wawona-vm-launch / UTM.
{
  pkgs,
  lib ? pkgs.lib,
  accel ? "auto",
  utm ? {
    dir = ../utm;
    qemuUtmPatch = ../utm/patches/qemu-10.0.2-utm.patch;
  },
}:

let
  _ = accel;
  _utm = utm;
  refuse = pkgs.writeShellScriptBin "wawona-qemu-android" ''
    echo "wawona-qemu-android retired. Use Relay static CPU / Mode B. No QEMU." >&2
    exit 1
  '';
  refuseLaunch = pkgs.writeShellScriptBin "wawona-vm-launch" ''
    echo "wawona-vm-launch retired. Use Relay. No QEMU." >&2
    exit 1
  '';
in
pkgs.symlinkJoin {
  name = "wwn-vms-android-relay-only";
  paths = [ refuse refuseLaunch ];
  postBuild = ''
    mkdir -p $out/share/wwn-vms
    cat > $out/share/wwn-vms/README <<'EOF'
    Product Android Linux guests start through Wawona Relay.
    This package only refuses retired QEMU helpers.
    EOF
  '';
  meta = {
    description = "Wawona Android VM leftovers: fail closed (Relay only, no QEMU)";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}

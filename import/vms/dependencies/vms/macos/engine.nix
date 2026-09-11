# macOS VM engine (imported leftover). Product path is Relay VZ
# (`wawona-vz-run` via `relay_start`). Never QEMU / wawona-vm-launch.
{
  pkgs,
  lib ? pkgs.lib,
}:

let
  refuse = pkgs.writeShellScriptBin "wawona-qemu-hvf" ''
    echo "wawona-qemu-hvf retired. Use Relay (wawona_relay / wawona-vz-run). No QEMU." >&2
    exit 1
  '';
  refuseLaunch = pkgs.writeShellScriptBin "wawona-vm-launch" ''
    echo "wawona-vm-launch retired. Use Relay (wawona_relay / wawona-vz-run). No QEMU." >&2
    exit 1
  '';
in
pkgs.symlinkJoin {
  name = "wwn-vms-macos-relay-only";
  paths = [ refuse refuseLaunch ];
  postBuild = ''
    mkdir -p $out/share/wwn-vms
    cat > $out/share/wwn-vms/README <<'EOF'
    Product macOS Linux guests start through Wawona Relay (Virtualization.framework).
    This package only refuses the retired QEMU + HVF helpers.
    EOF
  '';
  meta = {
    description = "Wawona macOS VM leftovers: fail closed (Relay only, no QEMU)";
    platforms = lib.platforms.darwin;
  };
}

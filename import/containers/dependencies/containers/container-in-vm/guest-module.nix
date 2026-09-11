# NixOS guest module: OCI-in-VM over virtiofs (Relay VZ / KVM).
# Prefer the integrated path in mobile/guest.nix. This module remains for
# composing extra container-only guests.
#
# Topology:
#   host Wawona compositor <- vsock + waypipe <- OCI Wayland client (crun)
#
# Host Relay materializes `config.json` + `rootfs/` and shares tag `oci-bundle`
# via Virtualization.framework virtiofs (never QEMU 9p).
{ config, pkgs, lib, ... }:

let
  vsockPort = 1024;
  bundleMount = "/run/wawona/oci-bundle";
in
{
  systemd.services.wawona-session.enable = lib.mkForce false;
  environment.systemPackages = with pkgs; [
    crun
    waypipe
  ];

  fileSystems.${bundleMount} = {
    device = "oci-bundle";
    fsType = "virtiofs";
    options = [
      "ro"
      "nofail"
    ];
  };

  systemd.services.wawona-container = {
    description = "Run the OCI Wayland client and forward it to Wawona";
    wantedBy = [ "multi-user.target" ];
    after = [ "local-fs.target" ];
    unitConfig = {
      ConditionPathExists = "${bundleMount}/config.json";
      RequiresMountsFor = bundleMount;
    };
    serviceConfig = {
      Restart = "on-failure";
      RestartSec = "3s";
      RuntimeDirectory = "wawona-container";
      StandardOutput = "journal+console";
      StandardError = "journal+console";
    };
    environment = {
      XDG_RUNTIME_DIR = "/run/user/1000";
    };
    script = ''
      set -euo pipefail
      work=/run/wawona-container/bundle
      upper=/run/wawona-container/upper
      overlay_work=/run/wawona-container/overlay-work
      mkdir -p "$work/rootfs" "$upper" "$overlay_work" "$XDG_RUNTIME_DIR"
      cp "${bundleMount}/config.json" "$work/config.json"
      if ! ${pkgs.util-linux}/bin/mount -t overlay overlay \
        -o "lowerdir=${bundleMount}/rootfs,upperdir=$upper,workdir=$overlay_work" \
        "$work/rootfs"; then
        echo "wawona-container: overlay mount failed" >&2
        exit 1
      fi
      printf 'WAWONA_RELAY_READY=1\n' > /dev/hvc0
      exec ${pkgs.waypipe}/bin/waypipe --vsock -s ${toString vsockPort} server -- \
        ${pkgs.crun}/bin/crun run --bundle "$work" wawona-oci
    '';
    postStop = ''
      ${pkgs.crun}/bin/crun delete --force wawona-oci >/dev/null 2>&1 || true
    '';
  };
}

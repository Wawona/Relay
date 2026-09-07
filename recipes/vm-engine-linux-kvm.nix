{
  pkgs,
  lib ? pkgs.lib,
}:

# Linux AppImage. KVM via cloud-hypervisor. Fail closed without /dev/kvm.
# Never QEMU. Never TCG.
assert pkgs.stdenv.hostPlatform.isLinux;

let
  ch = pkgs.cloud-hypervisor or null;
in
pkgs.writeShellApplication {
  name = "wawona-vm-launch";
  runtimeInputs = [ pkgs.coreutils ] ++ lib.optional (ch != null) ch;
  text = ''
    set -euo pipefail
    if [ ! -e /dev/kvm ]; then
      echo "wawona-relay: /dev/kvm missing. Fail closed. No QEMU TCG." >&2
      exit 2
    fi
    if ! command -v cloud-hypervisor >/dev/null 2>&1; then
      echo "wawona-relay: cloud-hypervisor not on PATH." >&2
      exit 2
    fi
    echo "wawona-relay: kvm-ch (cloud-hypervisor). Guest still planned until NixOS prebuilts wire here." >&2
    exit 2
  '';
}

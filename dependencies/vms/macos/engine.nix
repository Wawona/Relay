# macOS VM engine entry (Virtualization.framework via microvm.nix + vfkit).
#
# The runnable developer path lives in Wawona's flake apps `wawona-microvm` and
# `wawona-vm-bridge`; guest definition is `../microvm-guest.nix`. This package
# is the registry/build anchor for the macOS vm-engine slot.
{
  pkgs,
  lib ? pkgs.lib,
}:

pkgs.writeTextDir "README" ''
  wwn-vms macOS engine: microvm.nix + vfkit (Virtualization.framework).

  Developer flow (from Wawona repo):
    nix run .#wawona-microvm &
    nix run .#wawona-vm-bridge

  Guest: wwn-vms/dependencies/vms/microvm-guest.nix
  In-app track (future): WawonaLinuxVZ.swift + bundled guest artifacts.
''

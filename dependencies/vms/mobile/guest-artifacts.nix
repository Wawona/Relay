# Kernel + ext4 rootfs artifacts for the mobile QEMU-TCTI guest.
#
# Builds on aarch64-linux. NOTE: a *real* Linux builder (CI ubuntu-24.04-arm,
# see .github/workflows/guest-artifacts.yml) is required, NOT the Determinate
# native Linux builder on a case-insensitive macOS store: that builder shares
# /nix/store over virtiofs, exposing raw case-hack names (`l~nix~case~hack~1`,
# e.g. ncurses terminfo) which break make-initrd-ng and would poison the
# rootfs image (NixOS/nix#9319). The engine passes these artifacts as bundled
# / ODR data into the iOS app (never downloaded code).
{
  pkgs,
  lib ? pkgs.lib,
  mobileGuest,
  # ext4 root disk size for the trimmed NixOS closure. "auto" sizes to the
  # closure plus additionalSpace slack; a number means megabytes.
  diskSize ? "auto",
}:
let
  kernel = mobileGuest.config.boot.kernelPackages.kernel;
  makeDiskImage = import "${toString pkgs.path}/nixos/lib/make-disk-image.nix" {
    inherit pkgs lib;
    config = mobileGuest.config;
    inherit diskSize;
    format = "raw";
    label = "wawona-mobile-guest";
    installBootLoader = false;
    partitionTableType = "none";
    copyChannel = false;
  };
in
pkgs.runCommand "wawona-mobile-guest-artifacts" {
  passthru.kernel = kernel;
} ''
  mkdir -p $out
  if [ -f ${kernel}/zImage ]; then
    cp ${kernel}/zImage $out/zImage
  elif [ -f ${kernel}/bzImage ]; then
    cp ${kernel}/bzImage $out/vmlinuz
  elif [ -f ${kernel}/vmlinux ]; then
    cp ${kernel}/vmlinux $out/vmlinux
  else
    echo "No kernel image found in ${kernel}" >&2
    exit 1
  fi
  cp ${makeDiskImage}/nixos.raw $out/rootfs.img
  echo "kernel + rootfs.img for wawona-mobile-guest" > $out/README
''

# Kernel + ext4 rootfs artifacts for the mobile QEMU-TCTI guest.
#
# Builds on aarch64-linux (Determinate builder on macOS is fine). The engine
# passes these as bundled / ODR data into the iOS app (never downloaded code).
{
  pkgs,
  lib ? pkgs.lib,
  mobileGuest,
  # ext4 root disk size for the trimmed NixOS closure.
  diskSize ? "1024M",
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

# Android VM engine: QEMU with Android hypervisor (KVM) when `/dev/kvm` exists,
# otherwise TCG + JIT (Play-permitted).
#
# Accel selection is shared with crates/wwn-vms-engine (`AndroidHv` vs `TcgJit`).
# The QEMU binary/shared object is built from vendored UTM sources (../utm)
# via the NDK; until that cross-build lands this package ships the probe +
# launch wrapper and documents the JNI contract Wawona implements.
{
  pkgs,
  lib ? pkgs.lib,
  accel ? "auto",
  utm ? {
    dir = ../utm;
    qemuUtmPatch = ../utm/patches/qemu-10.0.2-utm.patch;
  },
}:

assert builtins.pathExists utm.qemuUtmPatch;

let
  launcher = pkgs.writeShellScriptBin "wawona-qemu-android" ''
    set -euo pipefail
    guest_dir="''${1:-}"
    memory_mb="''${2:-768}"
    qemu_bin="''${WAWONA_QEMU:-qemu-system-aarch64}"
    if [ -z "$guest_dir" ] || [ ! -d "$guest_dir" ]; then
      echo "usage: wawona-qemu-android GUEST_DIR [MEMORY_MB]" >&2
      exit 2
    fi
    rootfs="$guest_dir/rootfs.img"
    kernel=""
    for name in Image zImage vmlinuz vmlinux; do
      [ -f "$guest_dir/$name" ] && kernel="$guest_dir/$name" && break
    done
    if [ ! -f "$rootfs" ] || [ -z "$kernel" ]; then
      echo "guest incomplete under $guest_dir" >&2
      exit 1
    fi
    if [ -e /dev/kvm ]; then
      accel=kvm
      echo "[wawona-qemu-android] using Android hypervisor (/dev/kvm)" >&2
    else
      accel=tcg
      echo "[wawona-qemu-android] no /dev/kvm; TCG+JIT fallback" >&2
    fi
    if command -v wawona-vm-launch >/dev/null 2>&1; then
      exec wawona-vm-launch --guest-dir "$guest_dir" --memory "$memory_mb"
    fi
    exec "$qemu_bin" \
      -machine "virt,accel=$accel" \
      -cpu max \
      -m "$memory_mb" \
      -kernel "$kernel" \
      -drive "file=$rootfs,if=virtio,format=raw" \
      -device virtio-rng-pci \
      -nographic \
      -no-reboot
  '';
in
pkgs.symlinkJoin {
  name = "wwn-vms-android-qemu-hv";
  paths = [ launcher ];
  postBuild = ''
    mkdir -p $out/share/wwn-vms
    cat > $out/share/wwn-vms/README <<EOF
    wwn-vms Android engine (${accel}): QEMU + KVM when /dev/kvm exists, else TCG+JIT.

    Wawona JNI: nativeLaunchMobileVm probes /dev/kvm, then posix_spawns
    libqemu-system-aarch64.so (or qemu-system-aarch64) with matching -accel.

    UTM sources: ${toString utm.dir}
    Guest: ../mobile/guest.nix
    EOF
  '';
  meta = {
    description = "Wawona Android VM engine (QEMU + KVM/TCG)";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}

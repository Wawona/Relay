# macOS VM engine: QEMU + Hypervisor.framework (HVF).
#
# Default Machines path:
#   qemu-system-aarch64 -machine virt,accel=hvf  (Apple silicon)
#   qemu-system-x86_64  -machine q35,accel=hvf   (Intel)
#
# The Rust helper `wawona-vm-launch` (crates/wwn-vms-engine) is the preferred
# argv builder; this package also ships a shell fallback so the registry
# fragment builds without a Cargo.lock on every consumer.
#
# Legacy Virtualization.framework / vfkit remains in ../microvm-guest.nix and
# ../vz-launcher.nix for the developer microvm lane.
{
  pkgs,
  lib ? pkgs.lib,
}:

let
  qemu = pkgs.qemu;
  hostIsAarch64 = pkgs.stdenv.hostPlatform.isAarch64;
  qemuSystem =
    if hostIsAarch64 then "${qemu}/bin/qemu-system-aarch64"
    else "${qemu}/bin/qemu-system-x86_64";
  machine =
    if hostIsAarch64 then "virt,accel=hvf"
    else "q35,accel=hvf";
  launcher = pkgs.writeShellScriptBin "wawona-qemu-hvf" ''
    set -euo pipefail
    guest_dir="''${1:-}"
    memory_mb="''${2:-2048}"
    if [ -z "$guest_dir" ] || [ ! -d "$guest_dir" ]; then
      echo "usage: wawona-qemu-hvf GUEST_DIR [MEMORY_MB]" >&2
      echo "  GUEST_DIR must contain rootfs.img and Image|zImage|vmlinuz|vmlinux" >&2
      exit 2
    fi
    rootfs="$guest_dir/rootfs.img"
    if [ ! -f "$rootfs" ]; then
      echo "missing $rootfs" >&2
      exit 1
    fi
    kernel=""
    for name in Image zImage vmlinuz vmlinux; do
      if [ -f "$guest_dir/$name" ]; then
        kernel="$guest_dir/$name"
        break
      fi
    done
    if [ -z "$kernel" ]; then
      echo "no kernel under $guest_dir" >&2
      exit 1
    fi
    # Prefer Rust launcher when present on PATH (flake package wwn-vms-engine).
    if command -v wawona-vm-launch >/dev/null 2>&1; then
      exec wawona-vm-launch --guest-dir "$guest_dir" --memory "$memory_mb"
    fi
    echo "[wawona-qemu-hvf] QEMU + HVF (Hypervisor.framework) memory=''${memory_mb}M" >&2
    exec "${qemuSystem}" \
      -machine "${machine}" \
      -cpu host \
      -m "$memory_mb" \
      -kernel "$kernel" \
      -drive "file=$rootfs,if=virtio,format=raw" \
      -device virtio-rng-pci \
      -nographic \
      -no-reboot
  '';
in
pkgs.symlinkJoin {
  name = "wwn-vms-macos-qemu-hvf";
  paths = [ launcher qemu ];
  postBuild = ''
    mkdir -p $out/share/wwn-vms
    cat > $out/share/wwn-vms/README <<'EOF'
    wwn-vms macOS engine: QEMU + HVF (Hypervisor.framework)

      wawona-qemu-hvf /path/to/guest 2048

    Guest: Image (or vmlinux) + rootfs.img.
    GUI path: vsock + waypipe into Wawona (not RDP).
    EOF
  '';
  meta = {
    description = "Wawona macOS VM engine (QEMU + HVF)";
    platforms = lib.platforms.darwin;
  };
}

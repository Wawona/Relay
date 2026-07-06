# Android VM engine for wwn-vms.
#
# Android permits JIT, so unlike iOS this engine is NOT limited to TCTI:
#   * QEMU with TCG + JIT (fast software emulation), and
#   * opportunistic acceleration via the Android Virtualization Framework (AVF)
#     / KVM where the device+ROM expose it (Pixel 6+, GrapheneOS, etc.).
# Play-Store compliant (JIT is allowed on Android).
#
# The QEMU sources are the VENDORED UTM patchset in ../utm (same emulator as
# the Apple mobile path, built with JIT enabled for Android) cross-compiled
# through `wwn-toolchain`'s Android NDK toolchain. The guest is the same
# bundled minimal NixOS aarch64 image (../mobile/guest.nix).
{
  pkgs,
  lib ? pkgs.lib,
  # "qemu-jit" (portable, always available) or "avf" (needs device support).
  accel ? "qemu-jit",
  # Vendored UTM engine paths (wwn-vms `lib.utm` from the flake).
  utm ? {
    dir = ../utm;
    qemuUtmPatch = ../utm/patches/qemu-10.0.2-utm.patch;
    buildDependenciesScript = ../utm/scripts/build_dependencies.sh;
  },
}:

assert builtins.pathExists utm.qemuUtmPatch;

pkgs.writeTextDir "README" ''
  wwn-vms Android engine (${accel}): vendored UTM QEMU + mobile NixOS guest.

  Status: registry anchor only — cross-build through wwn-toolchain NDK is next.
  Guest: wwn-vms/dependencies/vms/mobile/guest.nix
  Sources: ${toString utm.dir}

  Wawona integration: MainActivity.kt VM lane → JNI loader for libqemu + guest
  artifacts (same kernel/rootfs.img as iOS). AVF/KVM used opportunistically;
  QEMU-JIT is the portable fallback.
''

# QEMU-TCTI mobile VM engine for iOS / iPadOS / visionOS / tvOS.
#
# On these targets there is no Hypervisor.framework, so the ceiling is the UTM
# SE model: jitless QEMU using TCTI (Tiny Code Threaded Interpreter) — an
# App-Store-approved precedent. The engine sources are VENDORED in-repo at
# ../utm (patches incl. qemu-10.0.2-utm.patch, build_dependencies.sh, reference
# backends) — see ../utm/README.md for provenance. No external UTM repo.
#
# Build inputs:
#   * ../utm vendored QEMU-TCTI patchset + dependency build machinery
#   * a bundled minimal NixOS guest (./guest.nix) as ODR/bundled data
#   * wwn-waypipe (vsock Wayland transport) for the guest->Wawona GUI bridge
#   * wwn-toolchain cross toolchains (buildForIOS/…)
#
# Until the full cross-build is wired, this evaluates cleanly (so the registry
# merges and target enumeration works) but fails the build with a precise
# message rather than pretending to produce an emulator.
{
  pkgs,
  lib ? pkgs.lib,
  # The Apple platform this engine targets (informational; the real derivation
  # picks the matching platform/arch slice in build_dependencies.sh).
  applePlatform ? "ios",
  # Vendored UTM engine paths (wwn-vms `lib.utm` from the flake). Defaults to
  # the in-repo location so this file also works via direct callPackage.
  utm ? {
    dir = ../utm;
    qemuUtmPatch = ../utm/patches/qemu-10.0.2-utm.patch;
    buildDependenciesScript = ../utm/scripts/build_dependencies.sh;
  },
}:

# Shape of the eventual derivation (guest + engine + transport). The real
# cross-build drives utm.buildDependenciesScript (`-p ios -a arm64` etc., TCTI
# scheme) under wwn-toolchain's Apple cross toolchains.
assert builtins.pathExists utm.qemuUtmPatch;
throw ''
  wwn-vms mobile engine (QEMU-TCTI) for ${applePlatform}: the vendored UTM
  sources are present (${toString utm.dir}) but the cross-build is not wired
  yet. Next: run utm.buildDependenciesScript through wwn-toolchain's
  ${applePlatform} toolchain (jitless TCTI configuration, no MAP_JIT), link the
  bundled NixOS guest (./guest.nix) + wwn-waypipe, and emit the embeddable
  engine framework. No JIT, no Hypervisor.framework — TCTI is the honest
  ceiling (COMPLIANCE.md).
''

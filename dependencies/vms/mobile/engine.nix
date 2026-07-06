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
# BUILD STATUS (2026-07-05): the engine cross-build is VERIFIED end-to-end on
# macOS 26 / Xcode 26 via the flake's `utm-engine` dev shell:
#   nix develop wwn-vms#utm-engine -c /bin/sh \
#     dependencies/vms/utm/scripts/build_dependencies.sh -p ios-tci -a arm64
# producing sysroot-iOS-TCI-arm64 (63 iOS frameworks: qemu-*-softmmu with
# tcg_threaded_interpreter=True, ANGLE, MoltenVK, spice, virglrenderer, ...).
# Requires Xcode's Metal toolchain (xcodebuild -downloadComponent MetalToolchain).
#
# This nix derivation is still a stub: wrapping that impure Xcode-driven build
# as a fixed-output/sandbox-relaxed derivation (or importing the prebuilt
# sysroot) is the remaining packaging step.
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
  wwn-vms mobile engine (QEMU-TCTI) for ${applePlatform}: the engine BUILD is
  verified (see header: `nix develop wwn-vms#utm-engine` + ios-tci arm64 gives
  the full framework sysroot), but it is Xcode-driven and not yet wrapped as a
  pure nix derivation. Run the dev-shell build, or wire the sysroot import
  here. Then link the bundled NixOS guest (./guest.nix) + wwn-waypipe into the
  embeddable engine framework. No JIT, no Hypervisor.framework — TCTI is the
  honest ceiling (COMPLIANCE.md).
''

# Per-target VM capability matrix + eval-time assertions ("capability lane").
# CI checks this with `nix eval .#lib.capabilities`; the asserts fail evaluation
# if the matrix ever drifts from the COMPLIANCE.md posture.
#
#   vm            can this target spawn a VM at all?
#   guestBundled  is the NixOS guest shipped as bundled/ODR data (Apple/Android)?
#   accel         "hvf" | "tcti" | "android-hv" | "kvm" | null
let
  caps = {
    macos = { vm = true; guestBundled = false; accel = "hvf"; };
    ios = { vm = true; guestBundled = true; accel = "tcti"; };
    ipados = { vm = true; guestBundled = true; accel = "tcti"; };
    tvos = { vm = false; guestBundled = false; accel = null; };
    visionos = { vm = false; guestBundled = false; accel = null; };
    watchos = { vm = false; guestBundled = false; accel = null; };
    android = { vm = true; guestBundled = true; accel = "android-hv"; };
    linux = { vm = true; guestBundled = false; accel = "kvm"; };
  };
  targets = builtins.attrNames caps;
in
# tvOS, watchOS, and visionOS never offer VM machine kinds.
assert !caps.tvos.vm && !caps.watchos.vm && !caps.visionos.vm;
# macOS: QEMU + Hypervisor.framework (HVF), not Virtualization.framework for Machines.
assert caps.macos.accel == "hvf";
# Apple mobile has no Hypervisor.framework -> jitless TCTI ceiling (no JIT in Mode A).
assert caps.ios.accel == "tcti" && caps.ipados.accel == "tcti";
# Android: prefer native HV (KVM); QEMU TCG+JIT is the portable fallback (same package).
assert caps.android.accel == "android-hv";
# Any target with a VM uses a host accelerator or bundles its guest as data.
assert builtins.all
  (t: !caps.${t}.vm || builtins.elem caps.${t}.accel [ "hvf" "kvm" ] || caps.${t}.guestBundled)
  targets;
caps

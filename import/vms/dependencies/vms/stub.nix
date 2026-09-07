# Port stub for the wwn-vms per-target VM engine / NixOS guest. Evaluates cleanly
# (so registryFragment merges and CI can enumerate the target) but fails the
# build with a clear message until the real engine lands. Replace with a proper
# per-platform derivation:
#
#   macos    -> microvm.nix + vfkit (Virtualization.framework) guest + runner
#   ios/etc  -> jitless QEMU-TCTI (UTM SE model) + bundled minimal NixOS guest
#   android  -> QEMU (TCG/JIT) or Android Virtualization Framework
#   watchos  -> N/A (no VM); see COMPLIANCE.md
{ ... }:
throw "wwn-vms: VM engine is not implemented yet (scaffold only). See README.md port plan and COMPLIANCE.md."

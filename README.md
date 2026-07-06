# wwn-vms

Wawona's **virtual-machine substrate**, split out of the Wawona repo so VM
support is developed, versioned, and CI'd independently and consumed by Wawona as
a flake input (like `wwn-weston`/`wwn-iland`/`wwn-waypipe`). Aligns with
`wwn-toolchain`.

The built-in VM is **NixOS-only**: wwn-vms ships prebuilt NixOS guest images and,
per target, the engine that boots them. The guest's Wayland session is forwarded
into Wawona over **vsock + waypipe** (no RDP, no emulated framebuffer for the GUI
path).

> **Status: SKELETON.** This repo currently provides the flake + `registryFragment`
> skeleton, this port plan, and `COMPLIANCE.md`. Build stubs
> (`dependencies/vms/stub.nix`) intentionally fail with a clear message. Real
> per-target engines are downstream.

## Engine per target

- **macOS** (direct/notarized, non-MAS): Virtualization.framework via
  [microvm.nix](https://github.com/microvm-nix/microvm.nix) + vfkit, plus the
  native `wawona-vz` Swift launcher. `writableStoreOverlay` + virtiofs ro-store
  guest (no `make-disk-image`/KVM). Rosetta for x86_64 guests. Requires the
  `com.apple.security.virtualization` entitlement; not Mac App Store viable.
- **iOS / iPadOS**: jitless **QEMU-TCTI** (UTM SE model) from the **vendored
  UTM engine sources in-repo** (`dependencies/vms/utm/`). No
  `Hypervisor.framework` on iOS, so TCTI is the ceiling; App-Store-approved
  precedent (UTM SE). NixOS `aarch64-linux` guest shipped as bundled / On-Demand
  Resources data - never downloaded executable code.
- **visionOS**: shares the iOS QEMU-TCTI path.
- **tvOS**: QEMU-TCTI with a minimal NixOS profile (tight RAM ceiling); may be
  management-only where a guest won't fit.
- **watchOS**: no VM (infeasible). See `COMPLIANCE.md`.
- **Android**: QEMU (TCG; JIT permitted on Android so faster than iOS) with
  opportunistic KVM / Android Virtualization Framework where a device exposes it.

### Making it fast on iOS

Honest ceiling is TCTI (no acceleration for store apps). Documented levers:
lightest NixOS profile, GUI over waypipe+vsock (not emulated GPU/framebuffer),
QEMU TCG tuning, warmed translation blocks. No JIT is attempted (App Store rule).

## Mobile engine (iOS / iPadOS / visionOS / tvOS)

- `dependencies/vms/mobile/guest.nix` - a bundled minimal NixOS aarch64-linux
  guest (headless cage + foot, waypipe over vsock, `pixman` software rendering,
  trimmed closure for the mobile RAM ceiling). Exposed as
  `nixosConfigurations.wawona-mobile-guest`; kernel/rootfs build on the
  aarch64-linux builder and ship as bundled / On-Demand-Resource **data**.
- `dependencies/vms/mobile/engine.nix` - the jitless **QEMU-TCTI** engine recipe.
  Built from the **vendored UTM sources** at `dependencies/vms/utm/` (exposed as
  `wwn-vms.lib.utm`: `qemuUtmPatch`, the dependency build scripts, reference
  backends, and the UTM SE Xcode scheme — see `dependencies/vms/utm/README.md`
  for provenance). The former separate `wwn-utm` repo/flake-input is gone; the
  engine unit lives in-repo. The recipe cross-compiles through `wwn-toolchain`;
  it still throws with precise next-steps until the full cross-build lands.
  TCTI is the honest ceiling (no Hypervisor.framework on iOS).

## Android engine

`dependencies/vms/android/engine.nix` - QEMU with **TCG + JIT** (JIT is allowed
on Android, so this beats the iOS TCTI ceiling) plus opportunistic AVF/KVM where
the device exposes it. Cross-compiled from the vendored UTM sources
(`dependencies/vms/utm/`) through `wwn-toolchain`'s Android NDK toolchain; boots
the same `mobile/guest.nix`. Play-Store compliant.

## Port plan

1. Consume `wwn-toolchain` cross toolchains (`buildForIOS`, `buildForMacOS`,
   `buildForAndroid`) and merge `registryFragment` into Wawona.
2. Relocate the working macOS path here: `microvm-guest.nix`, `vz-launcher.nix`,
   `WawonaLinuxVZ.swift` (from Wawona), keep flake apps `wawona-microvm` /
   `wawona-vm-bridge`.
3. Bring up the QEMU-TCTI engine from the vendored UTM sources
   (`dependencies/vms/utm/`) for iOS/iPadOS/visionOS/tvOS with bundled minimal
   NixOS guests.
4. Android engine (QEMU/AVF).
5. Replace `dependencies/vms/stub.nix` with per-platform derivations; expose
   `nixos-vm-{macos,ios,android,...}` and `vm-engine-*` packages.

## Convention

Follows the [wwn-* porting convention](https://github.com/Wawona/Wawona/blob/main/docs/2026-wwn-porting-convention.md).
See also Wawona `docs/2026-nixos-vm-bridge.md`.

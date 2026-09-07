# wwn-vms

[![CI](https://github.com/Wawona/wwn-vms/actions/workflows/ci.yml/badge.svg)](https://github.com/Wawona/wwn-vms/actions/workflows/ci.yml)
[![Guest artifacts](https://github.com/Wawona/wwn-vms/actions/workflows/guest-artifacts.yml/badge.svg)](https://github.com/Wawona/wwn-vms/actions/workflows/guest-artifacts.yml)

Wawona's **virtual-machine substrate**, split out of the Wawona repo so VM
support is developed, versioned, and CI'd independently and consumed by Wawona as
a flake input (like `wwn-weston`/`wwn-iland`/`wwn-waypipe`). Aligns with
`wwn-toolchain`.

The built-in VM is **NixOS-only**: wwn-vms ships prebuilt NixOS guest images and,
per target, the engine that boots them. The guest's Wayland session is forwarded
into Wawona over **vsock + waypipe** (no RDP, no emulated framebuffer for the GUI
path).

## Engine per target

| Host | Engine |
|---|---|
| **macOS** | **QEMU + HVF** (`Hypervisor.framework` via `-accel hvf`). Launcher: `wawona-qemu-hvf` / Rust `wawona-vm-launch`. |
| **iOS / iPadOS** | jitless **QEMU-TCTI** (UTM SE) from vendored `dependencies/vms/utm/`. Mode B IPA may enable JIT. |
| **visionOS / tvOS / watchOS** | VM machine kind forbidden by product policy. |
| **Android** | **QEMU + Android hypervisor** (`/dev/kvm` → `-accel kvm`) with **TCG+JIT** fallback. |
| **Linux** | QEMU + KVM where available. |

Legacy developer lane on macOS: Virtualization.framework via microvm.nix + vfkit
(`microvm-guest.nix`, `vz-launcher.nix`). Machines Start uses QEMU+HVF.

### Rust engine crate

`crates/wwn-vms-engine` selects accel and builds QEMU argv (C ABI for ObjC/JNI):

```text
wwn_vm_preferred_accel / wwn_vm_build_argv_json / wawona-vm-launch
```

### Making it fast on iOS

Honest ceiling is TCTI (no acceleration for store apps). Lightest NixOS profile,
GUI over waypipe+vsock, QEMU TCG tuning. No JIT in Mode A.

## Packages

```bash
nix build .#wwn-vms-macos-engine
nix build .#wwn-vms-android-engine
# Darwin + Xcode (impure):
nix build .#wwn-vms-mobile-engine-ios-tci
nix eval .#lib.capabilities
```

## Convention

Follows the [wwn-* porting convention](https://github.com/Wawona/Wawona/blob/main/docs/2026-wwn-porting-convention.md).
See `docs/MODE-A-B.md`, `COMPLIANCE.md`.

# wwn-vms: Mode A / Mode B implementation plan

Canonical product split: [Wawona `docs/mode-a-b.md`](https://github.com/Wawona/Wawona/blob/development/docs/mode-a-b.md).
iOS channels: [wawona-ios-mode-b-channels](https://github.com/Wawona/Wawona/blob/development/docs/agent-rules/wawona-ios-mode-b-channels.md).
Mirror: keep in sync with `Wawona/docs/vms-mode-a-b.md`.

## Goal

One Machines kind `virtual_machine`, **different backends per platform**, plus
Mode A vs Mode B on the iOS family:

| Platform | Mode A engine | Mode B / privileged |
|----------|---------------|---------------------|
| **macOS** | **QEMU + HVF** (`Hypervisor.framework`) | Same (macOS not App Store constrained) |
| **iOS / iPadOS** | UTM-SE-class **jitless** QEMU-TCTI | **JIT** UTM/QEMU via **TrollStore** and/or **Sileo** Mode B IPA |
| **tvOS / watchOS / visionOS** | VM machine kind forbidden | VM machine kind forbidden |
| **Android** | QEMU + KVM when `/dev/kvm` exists, else TCG+JIT | Root/privileged paths as designed |
| **Linux** | Host/QEMU profiles (TBD) | N/A |

### iOS QEMU: interpreter vs JIT

```text
App Store / TestFlight  →  -accel tcg / TCTI (UTM SE). Interpreter ceiling. No MAP_JIT.
TrollStore sideload     →  Mode B IPA with JIT (VMs + containers share engine).
Sileo (jailbreak)       →  Same JIT + Desktop / LockScreen / Swinging Bridge product.
```

Shared: Machines schema, guest artifacts, vsock + waypipe GUI, capability gates.
**Do not** assume the iOS interpreter path on macOS/Android or vice versa.

Containers are separate (`wwn-containers`).

## Shared substrate (both modes)

- Machine profile schema (`virtual_machine`)
- Guest image selection / NixOS guest artifacts (data)
- vsock + waypipe GUI path into Wawona
- Capability gate API: `VmEngineKind = .interpreterJitless | .jitEnabled`
- `crates/wwn-vms-engine`: `AccelKind::Tcti` vs `AccelKind::TcgJit`

## Mode A implementation

1. Embed only TCTI (UTM-SE) sources for store builds. Force `-accel tcg`.
2. CI: no JIT entitlements / `MAP_JIT` / Hypervisor / TrollStore/Sileo strings in store schemes.
3. Document TCTI ceiling; tune guest size.
4. Optional ODR UTM-SE payload. Still jitless data.

## Mode B implementation

1. Separate scheme `Wawona-iOS-ModeB` (name TBD). **Not** submitted to ASC.
2. JIT UTM/QEMU path (TrollStore entitlement and/or jailbreak).
3. `repo.wawona.io` auto Mode B IPA for Sileo.
4. Website documents TrollStore (JIT) and Sileo (full Mode B). Store never mentions either.

## Never

- Mode B engine inside App Store IPA behind a toggle.
- Jitless and JIT as one binary with an env var.
- VM machine kind on tvOS/watchOS/visionOS.
- Treating TrollStore as full Desktop/LockScreen/Swinging Bridge Mode B.

## Success

- Store IPA boots without JIT and passes review.
- TrollStore / Sileo Mode B IPA boots with JIT.
- Single Machines UI; engine selected by build flavor.

# Linux VMs in Relay. Mode A / Mode B

Canonical product split: [Wawona `docs/mode-a-b.md`](https://github.com/Wawona/Wawona/blob/development/docs/mode-a-b.md).
Engine: **Wawona Relay**. Not QEMU. Not UTM.

This file used to describe a QEMU / UTM-SE path. That is retired. Do not
implement `wwn-vms-engine` AccelKind or `wwn-qemu-run`.

## Goal

One Machines kind `virtual_machine`. Linux / NixOS prebuilts only. Guest GUI
is Wayland into Wawona (`wawona-guest-wayland-iland`).

| Platform | Mode A engine | Mode B / privileged |
|----------|---------------|---------------------|
| **macOS** | Virtualization.framework via Relay | Same plus desktop-host paths |
| **iOS / iPadOS** | Relay static / jitless CPU. Tipa proof presents a Relay SHM frame on IOMFB | Same Relay plus Mode B JIT CPU when MAP_JIT write+exec works. TXM EPERM = JIT ARMED + static CPU |
| **Android** | Relay static CPU. Planned. Fail closed | Root / privileged Relay. Planned |
| **Linux** | KVM via cloud-hypervisor or crosvm. Fail closed without `/dev/kvm` | N/A |
| **tvOS / watchOS / visionOS** | Forbidden | Forbidden |

Mode A vs Mode B is which binary was installed, not a Settings toggle.

Containers unpack OCI, then use the **same** Linux VM backend.

## Shared

- Machine profile schema (`virtual_machine` / `container`)
- Slim NixOS guest artifacts (data). Embed in the iOS tipa only after Relay frames
- vsock + waypipe into Wawona iland. IOMFB on TrollStore Mode B
- Capability gates. tvOS / watchOS / visionOS stay forbidden

## Never

- QEMU, TCTI, UTM, Spice, virgl, or `wwn-qemu-run`
- Mode B engine inside an App Store IPA behind a toggle
- VM machine kind on tvOS / watchOS / visionOS
- Document VM frames as done before Relay boots NixOS on that artifact

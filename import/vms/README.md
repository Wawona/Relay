# Linux guests for Relay

NixOS prebuilts and guest artifacts consumed by **Wawona Relay**.
Not a product VM engine. Relay picks the CPU.

Guest GUI is vsock + waypipe into Wawona (iland). Not RDP. Not Spice.

## Engine (Relay, not this tree)

| Host | Engine |
|---|---|
| **macOS** | Virtualization.framework via Relay |
| **iOS / iPadOS Mode A** | Relay static CPU. Planned. Fail closed. No QEMU |
| **iOS / iPadOS Mode B** | That CPU plus Relay JIT CPU. Planned. Fail closed. No QEMU |
| **visionOS / tvOS / watchOS** | VM machine kind forbidden |
| **Android Play** | Relay static CPU. Planned. No AVF. No QEMU |
| **Linux** | KVM via cloud-hypervisor or crosvm. Fail closed without `/dev/kvm` |

`crates/wwn-vms-engine` AccelKind / QEMU argv is leftover. Do not wire it
into Wawona. See [`docs/MODE-A-B.md`](docs/MODE-A-B.md).

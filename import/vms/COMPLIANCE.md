# wwn-vms App Store / platform compliance

Honest, per-target posture. Full Mode A/B plan: [`docs/MODE-A-B.md`](./docs/MODE-A-B.md)
and [Wawona `mode-a-b.md`](https://github.com/Wawona/Wawona/blob/development/docs/mode-a-b.md).

| Target | Mode A (store-shaped) | Mode B (privileged) | Notes |
| --- | --- | --- | --- |
| macOS (direct/notarized) | Virtualization.framework | Same + desktop-host SIP paths | Needs `com.apple.security.virtualization`. |
| macOS (Mac App Store) | **No VM run** | N/A in MAS | Image/docs only if ever exposed. |
| iOS / iPadOS | **jitless** QEMU-TCTI (UTM-SE model) | **JIT** UTM/QEMU in **Sileo Mode B IPA** only | Never link JIT into App Store IPA. |
| visionOS | **Forbidden** (product) | **Forbidden** | Native + remote only. |
| tvOS | **Forbidden** | **Forbidden** | Native + remote only. |
| watchOS | **No** | **No** | Infeasible. |
| Android | QEMU TCG (Play-safe) | Root/KVM paths optional | JIT OK on Android Play for QEMU. |

## Hard rules

- **Design Mode A and Mode B together**; ship only A to App Store.
- **No JIT on Apple Mode A.** TCTI only in store IPA.
- **Mode B IPA** is a separate artifact from `repo.wawona.io` automation.
- **Guests are data** on Apple Mode A (bundled/ODR) — no downloaded Mach-O.
- **GUI over waypipe + vsock**, not emulated GPU/framebuffer for the primary path.
- **visionOS/tvOS/watchOS:** no VM machine kind (Wawona product policy).

# Containers in Relay. Mode A / Mode B

Canonical product split: [Wawona `docs/mode-a-b.md`](https://github.com/Wawona/Wawona/blob/development/docs/mode-a-b.md).
Engine: **Wawona Relay**. Not QEMU. Not UTM. Not host Docker. Not proot.

This file used to describe UTM-SE / QEMU-TCTI container-in-VM paths. That is
retired. Do not implement proot or revive `wwn-qemu-run`.

## Goal

One Machines kind `container`. OCI unpack, then the **same** Linux VM backend
as `virtual_machine` (`relay-vm`). Guest GUI is Wayland into Wawona
(`wawona-guest-wayland-iland`).

| Platform | Mode A engine | Mode B / privileged |
|----------|---------------|---------------------|
| **macOS** | Apple Containerization via Relay (OCI on VZ) | Same plus desktop-host paths |
| **iOS / iPadOS** | Relay static / jitless CPU. Tipa proof: OCI tint on same SHM frame | Same Relay plus Mode B JIT CPU when MAP_JIT write+exec works |
| **Android** | Relay static CPU. Planned. Fail closed | Root / privileged Relay. Planned |
| **Linux** | Same KVM VM backend as `virtual_machine` | N/A |
| **tvOS / watchOS / visionOS** | Forbidden | Forbidden |

Mode A vs Mode B is which binary was installed, not a Settings toggle.

## Shared

- Machine profile schema (`container`)
- `relay-oci` prepare (userspace unpack). Never proot, never host Docker
- Slim NixOS / proof rootfs. Embed in the iOS tipa only after Relay frames
- vsock + waypipe into Wawona iland. IOMFB on TrollStore Mode B
- Capability gates. tvOS / watchOS / visionOS stay forbidden

## Relation to Wasm packages

`wpm` / `repo.wawona.io/wasm` installs **WASI modules for Wawona Runtime**.
That is **not** `container pull`. Both exist under Mode A. Mode B tipa may
JIT-execute the same `/wasm/v1` bytecode later; there is no Mode B wasm catalog.

## Never

- QEMU, TCTI, UTM, Spice, virgl, or `wwn-qemu-run`
- proot or host Docker as a product container backend
- Mode B engine inside an App Store IPA behind a toggle
- Container machine kind on tvOS / watchOS / visionOS
- Document container frames as done before Relay presents on that artifact

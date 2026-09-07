# Wawona Relay

L3′ engine Wawona calls for Linux guests and Mode A WASI.

Flake input name: **`wwn-relay`** (`github:Wawona/Relay/development`).

| Kind | What Relay runs |
|------|-----------------|
| `virtual_machine` | Linux / NixOS prebuilts only |
| `container` | OCI unpack, then the **same** Linux VM backend |
| WASI / `wpm` | Mode A bytecode (`/wasm/v1`) only. No Mode B wasm product |

## Backends (never QEMU, never UTM)

| Host | VM | Container | Wasm |
|------|----|-----------|------|
| macOS Apple silicon | `Virtualization.framework` | Apple Containerization (OCI on VZ Linux VMs) | Wasmtime Cranelift |
| Linux AppImage | KVM via cloud-hypervisor or crosvm. Fail closed without `/dev/kvm` | OCI-in-that-KVM-VM | Cranelift |
| iOS / iPadOS Mode A | Relay static CPU (planned until it boots NixOS) | OCI on that VM | Pulley / static |
| iOS / iPadOS Mode B (tipa / Sileo) | Mode A CPU plus JIT CPU | OCI on that VM | No distinct wasm product |
| Android Play / sideload Mode A | Relay static CPU. No AVF | OCI-in-VM. No proot | Bundled Cranelift or Pulley |
| Android root Mode B | AVF if granted, else KVM, else Mode A CPU | OCI-in-that-VM | No distinct wasm product |

Guest GUI is vsock + waypipe into Wawona (iland). Not Spice, virgl, or virtio-gpu into a second window.

Wawona is backend-blind: it sends a spec (`kind`, image, Mode A/B from the **artifact**). Relay returns a handle and a Wayland endpoint. No hidden in-app JIT toggle.

## Hard rejects

- QEMU (system, user, TCG, TCTI, HVF-via-qemu, `qemu-*.framework`)
- UTM / UTM SE, Spice, CocoaSpice, virgl
- Host Docker, runc-on-host, or proot as the product container runtime
- Windows / macOS / BSD / "any ISO" guests
- AVF in Play. Termux debs as a Relay backend

## Crates

```text
crates/relay-core   IR, spec, backend enum
crates/relay-vm     vz | kvm | avf-lab | static-cpu | mode-b-jit
crates/relay-oci    OCI pull / unpack (from wwn-oci)
crates/relay-wasm   WASI P1/P2 + wpm (from wwn-wasm)
crates/relay-ffi    C ABI for AppKit, UIKit, JNI, Linux GTK
```

Compat: `wawona_wasm_*` and historical `wwn_vm_*` symbols stay exported while L4 switches to `wawona_relay.h`.

Imported trees live under `import/{runtime,wasm,containers,vms}`. UTM and QEMU engine packages are not imported.

## Build

```bash
nix flake metadata
nix build .#wawona-wasm
nix build .#wawona-relay
cargo test --locked -p relay-core -p relay-vm -p relay-ffi
```

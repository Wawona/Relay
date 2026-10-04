# Retired sibling repos

`github.com/Wawona/wwn-vms`, `github.com/Wawona/wwn-containers`, and
`github.com/Wawona/wwn-wasm` were deleted on 2026-10-03. Relay is the VM,
container, and Wasm engine. Do not recreate those repositories. Do not send
new work there.

Local clones may remain on a developer machine, including unpushed CI-only
commits. Those clones are history, not a product input. Wawona's flake input
is `wwn-relay`.

## What moved

| Old repo | Where it lives now |
|---|---|
| `wwn-vms` NixOS guest, static CPU, VZ, KVM | `Relay/crates/relay-vm` and `Relay/import/vms` |
| `wwn-containers` OCI unpack and macOS Apple Containerization | `Relay/crates/relay-oci` and `Relay/import/containers` |
| `wwn-wasm` Pulley / Cranelift and `wpm` | `Relay/crates/relay-wasm` and `Relay/import/wasm` |

macOS containers stay Apple Containerization on a Virtualization.framework
Linux VM. The daemon, waypipe vsock bridge, and guest waypipe mount are
`Relay/import/containers/dependencies/containers/macos` (`wwn-containerd`,
`containerd-bridge.nix`, `waypipe-guest-vsock.nix`). Wawona bundles
`wwn-containerd` from the Relay package. That is the container backend. It is
not host Docker, runc-on-host, or proot.

## What was not ported

QEMU, UTM, TCTI, and HVF-via-qemu. That includes the untracked local crates
`wwn-vms/crates/wwn-qemu-run` and `wwn-vms/crates/wwn-vsock-peer` (a
vhost-user-vsock peer for QEMU). Relay guest GUI is vsock plus waypipe into
Wawona, not a QEMU chardev.

Android proot was a `wwn-containers` sketch. Play containers are OCI-in-VM.
No proot.

# Relay product matrix

Canonical rules: `wawona-linux-vms-relay-runtime`, `wawona-guest-wayland-iland`,
`wawona-relay-wasm`, `wawona-mode-a-b`.

Relay is Rust-first. Product crates build with **crate2nix** from the
`wwn-toolchain` flake input (`crate2nix.follows = "wwn-toolchain/crate2nix"`)
so each crate is a separate Nix store path. No new monolithic
`buildRustPackage` workspace recipes. No new Swift/C engine logic beyond thin
ABI trampolines.

Wawona Machines sends `{ kind, image, artifact-class }`. Relay picks the
backend. Kind is `vm` | `container` | `wasm`. Artifact-class is which
binary the user installed (Mode A store/Play vs Mode B tipa/Sileo/desktop-host/root).

Containers always mean: unpack OCI, then the same Linux VM backend as `vm`.

## Supported targets

- macOS Apple silicon: Virtualization.framework VM lifecycle is implemented
  and boot-verified for the 4 KiB and 16 KiB NixOS guests (Linux 7.2+,
  `CONFIG_ARM64_16K_PAGES` for the latter). Machines UI integration and guest
  OCI lifecycle remain planned.
- Linux: KVM through cloud-hypervisor or crosvm is planned and fails closed
  without `/dev/kvm`.
- iOS and iPadOS: the Relay static AArch64 CPU is planned. Mode A never gains
  JIT or a hidden runtime switch.
- Android: the store static CPU is planned. Root-only AVF remains outside Play.
- tvOS, watchOS, and visionOS: VM and container execution are forbidden.
- WASI: Relay runs on every product target. Apple mobile uses Pulley. macOS and
  Linux use Cranelift.

## Machine and guest records

`RelayMachineProfile` version 1 is the canonical VM/container profile. It owns
the stable machine ID, kind, guest manifest, memory, disk limit, and OCI image
selection. Native UI must not duplicate these validation rules.

`GuestManifest` version 1 carries AArch64 Linux compatibility metadata, page
geometry, memory size, command line, and size plus SHA-256 for every artifact.
Release starts require a trusted Ed25519 signature over canonical manifest
JSON. An unsigned manifest runs only when native packaging explicitly marks
development resources as unsigned.

Runtime paths are separate from persisted profiles. Native packaging supplies
the VZ launcher, state directory, optional guest bundle directory, and trusted
guest public keys. These paths never select a backend.

## Container guest protocol

Relay validates the OCI index, exact Linux platform, image config, compressed
blob digest, uncompressed layer diff ID, tar paths, whiteouts, and special-file
policy before delivery. The version 1 guest protocol frames bounded lifecycle
control requests and exposes chunked reads only for blobs in that validated
image plan. Guest agent wiring, isolation, networking, and end-to-end OCI smoke
remain planned.

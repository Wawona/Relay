# Wawona Relay

L3′ engine Wawona calls for Linux guests and Mode A WASI.

Flake input name: **`wwn-relay`** (`github:Wawona/Relay/development`).

| Kind | What Relay runs |
|------|-----------------|
| `virtual_machine` | Linux / NixOS prebuilts only |
| `container` | OCI unpack, then the **same** Linux VM backend |
| WASI / `wpm` | `/wasm/v1` bytecode. Mode B may execute it (Pulley on Apple mobile). No Mode B catalog |

## Backends (never QEMU, never UTM)

| Host | VM | Container | Wasm |
|------|----|-----------|------|
| macOS Apple silicon | `Virtualization.framework` | Apple Containerization (OCI on VZ Linux VMs) | Wasmtime Cranelift |
| Linux AppImage | KVM via cloud-hypervisor or crosvm. Fail closed without `/dev/kvm` | OCI-in-that-KVM-VM | Cranelift |
| iOS / iPadOS Mode A | Relay static CPU (jitless AArch64) | OCI on that VM | Pulley / static |
| iOS / iPadOS Mode B (tipa / Sileo) | Same CPU plus JIT when MAP_JIT write+exec works | OCI on that VM | Same bytecode. Pulley until MAP_JIT |
| Android Play / sideload Mode A | Relay static CPU. No AVF | OCI-in-VM (virtiofs `oci-bundle` + guest crun). No proot | Pulley |
| Android root Mode B | AVF if granted, else KVM, else Mode A CPU | OCI-in-that-VM | Cranelift |

Guest GUI is vsock + waypipe into Wawona (iland). Not Spice, virgl, or virtio-gpu into a second window.

Wawona is backend-blind: it sends a spec (`kind`, image, Mode A/B from the **artifact**). Relay returns a handle and a Wayland endpoint. No hidden in-app JIT toggle.

## Language and Nix builds

Relay product code is **Rust**. C ABI headers and thin Apple framework
trampolines are allowed; do not grow Swift/C engines. Nix builds of Relay
crates must use **crate2nix** (per-crate store paths) pinned from L0:

`crate2nix.follows = "wwn-toolchain/crate2nix"`.

Do not add new whole-workspace `buildRustPackage` recipes. Host macOS/Linux
`wawona-relay` builds via `recipes/relay-crate2nix.nix`. Apple-mobile and
Android stay on `relay-staticlib.nix` until that cross path is rewritten.
Open debt: replace Swift `wawona-vz-run` with a Rust Virtualization.framework
launcher.

## Current implementation

- Relay owns a versioned VM/container Machine profile. It validates identifiers,
  memory, guest geometry, OCI image requirements, and stopped-only grow-only
  disk limits before producing a start spec.
- Guest manifests publish architecture, OS, minimum Relay version, artifact
  sizes, and SHA-256 hashes. Release starts require a trusted Ed25519
  signature. Unsigned manifests require an explicit development resource flag.
- Static CPU: proof ELF, bounded guest memory, boot artifact staging, MMU fault
  scaffolding, timer state, and a Virtio 1.0 MMIO split queue. Linux boot is
  still planned. `PageTranslate` maps 4 KiB or 16 KiB guests onto the host
  process page size without a second VM product.
- Mode A benches: `nix run .#relay-mode-a-bench` (see live charts above).
- macOS VZ: the native `wawona-vz-run` launcher boots the generated Linux 7.2
  NixOS bundle through stage 1 and stage 2 to the automatic `wawona` login, and
  starts the guest Wayland service. Relay Rust owns artifact resolution,
  verification, writable disk cloning, process state, console logs, readiness,
  Wayland endpoint creation, graceful stop, and forced-stop recovery. Today's
  `wawona-vz-run` is still a Swift Virtualization.framework trampoline; replace
  it with Rust (ObjC only if the framework ABI forces a thin bridge).
- Virtio block: immutable base plus sparse writable overlay, validated
  descriptor chains, queue notification, used-ring completion, and interrupt
  acknowledgement.
- OCI: host materializes a crun runtime bundle (`config.json` + `rootfs/`) from
  a validated local image-layout (or a slim readiness bundle). macOS VZ shares
  it over virtiofs tag `oci-bundle`. The NixOS guest mounts that tag, runs
  `crun` under waypipe on vsock port 1024, and keeps Foot only when the share
  is absent. No host runc, no proot, no QEMU 9p.
- WASI: `relay_start` validates modules, then executes via linked
  `wawona_wasm_run` (same process as `libwawona_wasm.a`) or `WAWONA_WASM` /
  `wasm` on PATH. Pulley on Apple mobile and Android Mode A. Cranelift on
  macOS, Linux, and Android Mode B. Bare `/wasm/v1` package names stay planned
  until wpm resolution is wired into Relay.
- Linux KVM starts fail closed until their real launch adapters exist. Relay
  never returns a success-shaped VM handle from a placeholder.

## Mode A speed (live)

Charts are published by GitHub Actions to the floating `bench-latest` release.
README markdown does not change when numbers update.

**1A:** CI uses engine CLIs + in-process class refs (not App Store `.app` binaries).
**2A:** Mode A gate = App Store-class interpreters only.
**2B:** A second chart includes JIT UTM and is labeled cross-class (not the gate).

![Mode A vs App Store-class interpreters](https://github.com/Wawona/Relay/releases/download/bench-latest/mode-a-interpreters.svg)

![Mode A page geometry](https://github.com/Wawona/Relay/releases/download/bench-latest/mode-a-page-geom.svg)

![Cross-class including JIT UTM](https://github.com/Wawona/Relay/releases/download/bench-latest/cross-class-incl-jit-utm.svg)

[![Mode A bench gate](https://img.shields.io/endpoint?url=https://github.com/Wawona/Relay/releases/download/bench-latest/gate.json)](https://github.com/Wawona/Relay/releases/tag/bench-latest)
[![Mode A marketing eligibility](https://img.shields.io/endpoint?url=https://github.com/Wawona/Relay/releases/download/bench-latest/marketing.json)](https://github.com/Wawona/Relay/docs/mode-a-bench.md)

Methodology: [`docs/mode-a-bench.md`](docs/mode-a-bench.md). Do not quote the
cross-class chart as Mode A proof. Do not say “world’s fastest” until
`marketing.json` is `fastest-eligible` (5 consecutive gate passes).

```bash
nix run .#relay-mode-a-bench -- --out /tmp/relay-bench
# or: cargo run -p relay-bench --release -- --out /tmp/relay-bench
```

## Page geometry

One StaticCpu backend maps **4 KiB or 16 KiB** guest kernels onto the host
process page size (`PageTranslate`). Guest Images stay real 4k / 16k builds.
See Wawona `docs/relay-page-geometry.md`.

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
nix build .#wawona-nixos-guest-4k
nix build .#wawona-nixos-guest-16k
cargo test --locked -p relay-core -p relay-vm -p relay-ffi
```

The macOS lifecycle smoke uses the same Rust start/ready/stop path as the C ABI:

```bash
nix build .#wawona-nixos-guest-4k -o result-guest
nix build .#wawona-vz-run -o result-vz
cargo run -p relay-vm --example vz_smoke -- \
  ./result-vz/bin/wawona-vz-run ./result-guest ./relay-vm-state
```

The NixOS guest derivations use Linux 7.2 or newer and emit `Image`, `initrd`,
`rootfs.img`, `cmdline`, and `manifest.json`. They build on Determinate's native aarch64-linux builder
even when the macOS Nix store is case-insensitive. The guest uses a minimal
scripted initrd with no case-colliding terminfo data. Rootfs construction
removes Nix case-hack suffixes inside the ext4 image and validates the result.
The 16 KiB guest builds a real `CONFIG_ARM64_16K_PAGES=y` Linux 7.2 kernel from
the NixOS config (ACPI/PCI for Apple VZ), with unused trees and loadable
modules trimmed so the builder scratch disk does not fill. Both 4 KiB and
16 KiB bundles are boot-proven through `wawona-vz-run`.

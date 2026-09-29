# Relay VM proof obligations

This register states exactly what is proved and what is not. A passing gate is
evidence for these rows only. It is not a whole-VM correctness claim.

| Law | Production Rust | Bounded proof | Unbounded model | Runtime evidence |
|---|---|---|---|---|
| Host arena covers guest RAM, is host-page aligned, and pads by less than one host page | `page_translate::round_up_host_arena` | `host_arena_rounding_covers_aligned_guest_ram` | `guest_4k_on_host_16k_rounding`, `naturally_aligned_page_pairs` | All four 4/16 KiB pairs and rejection tests |
| Split low/high MMIO writes preserve the untouched half and assemble the requested 64-bit value | `virtio_mmio::set_low`, `virtio_mmio::set_high` | `queue_address_halves_reconstruct_every_u64` | Bit-vector identity is checked on production code by Kani; no separate integer model | MMIO queue configuration tests |

## Preconditions

- Guest and host page sizes are validated 4 KiB or 16 KiB values.
- MMIO split writes use two 32-bit register values for one 64-bit address.

## Not proved

- Whole Linux boot, guest kernel correctness, or device-driver correctness.
- Liveness, fairness, scheduling, interrupt delivery timing, or absence of
  deadlock across the whole VM.
- Virtualization.framework, KVM, Hypervisor.framework, Metal, Swift, Kotlin,
  Objective-C, JNI, or C implementation correctness.
- Compiler, linker, operating system, hardware, or supply-chain correctness.
- Equivalence between the Verus mathematical model and every future production
  refactor. Review must maintain the named pairing.

Fuzzing, Miri, sanitizers, Clippy, OSV, cargo-deny, CodeQL, and tests add
independent evidence. They do not expand the mathematical proof boundary.

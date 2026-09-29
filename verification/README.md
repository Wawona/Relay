# Relay formal-verification gate

Relay uses both verifiers as mandatory, complementary build gates:

- Kani 0.68.0 checks the production Rust helpers for page-arena rounding and
  split 64-bit MMIO addresses.
- Verus 0.2026.09.27.3cf1832 proves the corresponding unbounded page-layout
  invariants in `verus/relay_vm_invariants.rs`.

Run `scripts/verify-formal.sh`. A Relay command should be launched through
`scripts/verified-cargo.sh`; it executes both proof suites first and refuses to
run the requested Cargo command if either verifier is missing or fails.
The workspace Cargo runner independently compares the proof stamp with a digest
of the current VM and specification sources, so direct `cargo run` and
`cargo test` execution also fail closed when proofs are absent or stale.

The verifiers are host-side gates. They are not linked into App Store iOS,
iPadOS, or visionOS artifacts. Production VM code remains the Rust checked by
Kani, while Verus specifications stay outside the target dependency graph.

Named laws, their production functions, proof harnesses, models, concrete
tests, preconditions, and unproved boundaries are registered in
`PROOF_OBLIGATIONS.md`. This is Wawona's Rust-native adoption of the useful
LAWs idea. Relay does not compile a Bend-2 DSL to C: that would duplicate
product logic and would not be covered by the shared Rust semantics of Kani,
Verus, and Miri.

Run `scripts/verify-quality.sh` for the full local gate: rustfmt, Clippy with
correctness and suspicious findings denied, Kani, Verus, strict-provenance
Miri, then locked workspace tests. Other Clippy findings remain visible while
legacy warning debt is retired deliberately. CI also runs the page-translation
libFuzzer target, OSV at the organization baseline, and cargo-deny for
advisories, licenses, bans, and source policy. Add a law to the obligation
register only when its production helper, bounded harness, and concrete tests
ship together.

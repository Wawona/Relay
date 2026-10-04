# Fixed EL1 hardware reference

`hardware.rs` is a standalone macOS Apple Silicon development tool, compiled
with rustc and ad-hoc signed for Hypervisor.framework by
`scripts/verify-el1-reference.sh`. It is not a Cargo workspace member, product
backend or bundled dependency. It accepts no guest code or image input.

`cases.rs` constructs fixed inputs shared with the interpreter test. The native
runner independently executes those inputs on hardware and writes observations;
it never calls StaticCpu or computes expected architectural results. The normal
portable test compares StaticCpu against the recorded `reference.jsonl`.

Run `scripts/verify-el1-reference.sh` to verify that fresh native observations
match the fixture. Use `--update` only to intentionally regenerate observations,
then rerun `scripts/verified-cargo.sh test -p relay-vm hardware_el1_checkpoints`.
A timeout, framework failure, unexpected trap, missing observation or divergence
fails the gate. HVC exits advance native PC; the recorder subtracts exactly four
bytes to compare the pre-HVC boundary used by the interpreter. Every case gets
a fresh VM: reusing translations after directly replacing page tables made an
early prototype produce a stale-TLB artifact when switching granules.

Current inputs: 29 cases on each of 4 KiB and 16 KiB granules. They cover leaf
PXN/UXN, EL1 user-writable execute prohibition, WXN, software access flags,
AF-before-permission priority, read/write permission faults, successful read,
write and execute, EL0 SVC entry into the EL1 vector, EL1 SVC from SP0/SPx, inherited AP/XN restrictions, effective
EL1 execution permissions after table restrictions, and invalid level-3 page
descriptors. Injected IRQs cover EL0 and EL1 SP0/SPx entry. ERET covers
EL0 and both EL1 stack selections. A masked expired virtual timer checks that
ISTATUS remains set independently of IMASK. Compare PC, X0,
PSTATE, the active stack and both stack banks, SPSR_EL1, ELR_EL1, ESR_EL1, FAR_EL1 and the target
memory word. All inputs are
single-vCPU and little-endian with no device accesses. Table-restriction
cases isolate the target in a separate level-3 table; code/vectors retain
independent executable mappings.

This is a bounded micro-guest reference, not full Linux trace equivalence,
ISA completeness, a hardware proof, or release attestation. MMIO, interrupt
priorities, all descriptor formats and general asynchronous exception timing remain
outside this corpus. The fixture is reviewable evidence, not a signed artifact.

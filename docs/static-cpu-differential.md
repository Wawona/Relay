# StaticCpu differential execution

This harness replaces blind missing-opcode iteration with measured state
comparison. It is development tooling, not a QEMU product path.

## Trace contract

Each JSON-lines record is one checkpoint after an exact interpreter-step
count. Schema version 1 contains:

- PC and translated physical PC
- X0 through X30, SP, SP_EL1, NZCV, and V0 through V31
- EL1 translation, exception, thread, interrupt-mask, FP, and timer state
- SHA-256 for every guest-physical page written since the prior checkpoint
- a state hash and cumulative chain hash

All 64-bit and 128-bit architectural values use fixed-width hexadecimal
strings. Dirty pages are ordered by ascending guest-physical address. A
reference recorder must use the same ordering and must start after boot
artifacts are staged, before the first guest instruction.

The cumulative hash makes divergence monotonic. Once one checkpoint differs,
every later chain hash differs. The comparator therefore binary-searches the
first divergent checkpoint. Record the surrounding range again with a smaller
interval until one instruction remains, then add that instruction sequence as
a minimized regression test.

## Relay trace

Run through the verified Cargo wrapper so Kani and Verus gate the current VM
source first:

```bash
scripts/verified-cargo.sh run -p relay-vm --example static_differential -- \
  record GUEST_DIRECTORY relay.jsonl 100000 10000000
```

## Reference trace

Use native AArch64 or QEMU only outside the product. The reference must use the
same guest artifacts, virtual platform, deterministic device inputs, timer
model, and interpreter-step boundaries. Convert its checkpoints to the
schema above. Never link or bundle QEMU, TCG, or its plugins into Relay or a
Wawona app artifact.

## Compare

```bash
scripts/verified-cargo.sh run -p relay-vm --example static_differential -- \
  compare reference.jsonl relay.jsonl
```

A nonzero exit reports the checkpoint index, instruction counts, and first
high-level mismatch. This does not prove whole-VM correctness. It locates the
first measured architectural disagreement for diagnosis and regression work.

## Evidence validation

The reader verifies each state hash and its cumulative chain, contiguous
zero-based sequence numbers, increasing step counts, register widths, and
ordered unique dirty-page addresses. Empty, blank, malformed, or tampered
traces fail instead of reporting a match. Hash integrity is not a signature
or proof that a reference recorder is correct. A shared truncated prefix is
not a complete boot proof; callers must also require the intended endpoint.

The `instructions` field counts interpreter steps, including steps that enter
synchronous exceptions, and timer/IRQ processing follows each step. It is not
an architectural retired-instruction counter. An external reference adapter
must reproduce this boundary convention before whole-boot traces can be
compared meaningfully. No full-system reference adapter is shipped yet.

## Native instruction-family corpus

On an AArch64 host, `cpu/native_reference.rs` executes fixed, statically assembled
reference instructions and compares interpreter results. No guest bytes execute
on the host. FP cases save and restore host FPCR/FPSR (and NZCV when comparing
flags), exercise guest rounding/FZ/DN controls, and compare cumulative exceptions
as well as destination bits. Corpus families include lane extraction/broadcast,
float precision/integer conversions, scalar arithmetic/comparison, and vector
integer arithmetic/comparison/reduction/bitwise operations. Reserved encodings
must fail without mutating destination, PC, or flags.

```bash
scripts/verified-cargo.sh test -p relay-vm cpu::native_reference
```

Native-assembly cases are disabled under Miri and on other host architectures.
Portable boundary regressions remain available there. The software arithmetic
wrapper uses pinned `rustc_apfloat`, handles ARM NaN priority and flushing, and
corrects the library's missing overflow flag for directed saturation. This
corpus exposed pre-existing host-cast/host-compare exception-flag bugs. It does
not replace the still-missing whole-system reference recorder.

## Bounded kernel probes

For a boot wait, use the opt-in `kernel-probe` development feature:

```sh
cargo run --release -p relay-vm --features kernel-probe \
  --example static_kernel_probe -- GUEST_DIRECTORY CONFIG.json OUTPUT.jsonl
```

The JSON config contains `instructions` (positive instruction budget), `pcs`
(up to 32 numeric virtual PCs from the exact kernel's System.map) and `addresses`
(up to 16 numeric virtual RAM addresses). Output creation refuses overwrites.
Each selected entry records registers, SP, SP_EL0, ELR_EL1, 32 words at each
watched address and selected register pointer, and one level of the first four
kernel-pointer words. `registers` selects up to eight X-register indices (default
X0/X1/X2); `register_offsets` adds up to eight checked byte offsets from each
selected pointer. `sample_every_hits` optionally adds a periodic hit stride
(at least 16). Unmapped/device words are
null. Reads use GuestMemory directly and do not acknowledge interrupts, consume
console data or install guest faults. Samples occur at the first 16 hits and
subsequent powers of two, plus every 100 million instructions. This is diagnostic
sampling, not an instruction-complete reference trace or proof of readiness.

The feature is disabled by default; no probe loop enters ordinary app builds.
Use exact kernel symbols (and account for relocation when applicable). Never
infer a missing callback from a sample set without checking the selected entry PC.

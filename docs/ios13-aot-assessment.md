# Mode A guest runtime: offline AOT

Status: 2026-10-04. The offline image, signed-byte check, software TLB,
page invalidation, and one host thread per vCPU are implemented in
`crates/relay-vm/src/aot.rs`. The translator is feature `aot-translate`
(and tests). It is not a default feature and does not ship in the store
staticlib. Translated blocks call `StaticCpu::execute_fetched`. A changed
guest byte or a store into an executed page falls back to `StaticCpu::step`.

A three-instruction block runs with zero fallback and matches the oracle.
Two vCPUs run that path on two host threads. The bundled NixOS image is
not translated. There is no speed measurement and no fastest or cleanest
claim. Shared guest RAM across those threads is a mutex `SharedRam` with a proved
span check. Each vCPU job still owns a private StaticCpu arena. Full SMP on
one `GuestMemory` is unproved. Float, MMU, exceptions, and virtio are not
reimplemented in the translator.

## Target

Mode A is designed to be the fastest App Store-safe ARM64 Linux virtual
machine, in the smallest codebase that can be proved. "World's fastest" and
"cleanest" are targets. Neither is a claim until the measurement contract
below has been run and the proof obligations below name what was proved.

A single-threaded interpreter cannot be that runtime. StaticCpu remains the
semantic oracle and the fallback for guest bytes that are not in the signed
translation. It is not the steady-state engine for the bundled NixOS image.

## What runs today

Relay's Rust runtime is compiled ahead of time. Linux ARM64 guest instructions
are interpreted by one StaticCpu thread. Store-mode Wasm uses Pulley. iOS and
iPadOS 27+ Mode A Wasm may use Wasmer WASIX when WasmerSDK is linked. That is
not the VM CPU. No guest-native AOT backend exists. Optimizing the interpreter
binary does not make guest execution AOT. The bundled NixOS image still has
open boot and graphics gates. On 2026-10-03 a 4 KiB simulator boot ran
12,691,156,992 instructions in 897 seconds and stopped on unimplemented
`fsqrt d0, d0`. On 2026-10-05 a 4 KiB StaticCpu guest reached systemd
and printed `WAWONA_RELAY_READY=1` (guest ~504s). Offline AOT of the
three-instruction MOVZ/ADD/RET closure still matches StaticCpu with
0 fallback instructions. The bundled NixOS image is not translated.
There is no speed measurement and no fastest or cleanest claim.

## Engine

Everything below stays inside the signed app. No runtime compiler. No writable
executable pages. No `MAP_JIT`. No Hypervisor.framework. No Cranelift. No
second CPU implementation.

1. One semantics. StaticCpu's pure helpers are the architecture. Offline AOT
   emits calls to those helpers, or a translation proved equivalent to them.
   Float, MMU, exceptions, and virtio are not reimplemented in the translator.
2. Offline AOT. The build translates the exact bundled kernel and executable
   closure into host functions and a dispatch table bound to image hashes and
   original bytes. The translator is parallel. It does not ship in the app.
   A changed guest byte cannot reuse a stale translation.
3. Multi-threaded execution. One host thread per guest vCPU. Translated blocks
   from different vCPUs run concurrently. Block, console, and vsock completion
   stay off the vCPU threads. Guest SMP is in scope for the benchmarked image.
4. Fast guest memory. A software TLB and checked regions sit on the translated
   path. Guest pages are never mapped executable.
5. Signed indirect branches. A table built offline selects the next host
   function. That is not a JIT.
6. Hybrid edge. Bytes missing from the translation (self-modified code, a
   loader the closure did not include, an OCI program downloaded after
   signing) run on StaticCpu, or fail closed in a closed-world product mode.
   The benchmarked NixOS boot, login, and Wayland session must be on the AOT
   path, not the fallback.

Linux ARM64 machine code still cannot run as iOS user code. Exceptions,
sysregs, and virtio keep StaticCpu's semantics. AOT removes decode and retires
straight-line guest code as host code that calls the shared helpers.

Arbitrary OCI programs downloaded after the app is signed have no host
translation in that signature. A closed-world image and "any Linux program"
are different contracts. Do not silently drop either.

LLVM ORC and JITLink are not this store path. They are a runtime compiler.

## Proof

This is Tier 3 work: guest CPU, MMU, and virtio. Kani checks the production
helpers. Verus holds the unbounded model of the refinement: for every guest
instruction in the advertised base profile, the translated block matches
StaticCpu's helpers on registers, flags, faults, and memory effects. Native
differential tests stay. Miri and sanitizers stay. None of them is a
whole-VM proof.

Named obligations, not a blanket "proved":

- Instruction refinement for the translated closure.
- Image identity: dispatch refuses a hash or byte mismatch.
- Invalidation: a guest store to an executed page drops that translation and
  returns to StaticCpu.
- vCPU interference: the memory model for two host threads translating loads
  and stores through the software TLB.

State the unproved remainder in the same change. Scheduling fairness, guest
timing, and the full NixOS image are not implied by a helper proof. Do not
add a second verified implementation. Do not generate a C product CPU.

## Comparison set

Publish two classes separately.

Store-legal class: other App Store-safe ARM64 Linux VMs, interpreters and
offline translators. This is the class the design is built to win.

Hardware class: Hypervisor.framework, KVM, UTM, and any JIT that needs
`MAP_JIT`. Mode A is not allowed to use those mechanisms. Their numbers are
recorded so a later claim is honest. They are not a reason to put JIT or a
hypervisor in the store IPA.

Same device, OS, guest image, RAM, disk, power, and thermal start. Cold boot
to `Reached target Multi-User System`, first real Wayland frame, application
launch, interactive latency, steady CPU, peak RAM, signed app size, and
energy. Repetitions, medians, tails, and failures. Also record translator
time, signed text growth, and how many guest bytes fell back to StaticCpu.

No "world's fastest" or "cleanest" sentence in a release, the App Store
listing, or a status line until that table exists.

## Adoption order

1. StaticCpu must execute the advertised base profile. A missing instruction
   such as `FSQRT` is an oracle bug, not AOT work.
2. Repeatable boot to Multi-User and a real frame on the interpreter, both
   page sizes.
3. Profile that boot. Translate the hottest closure first, still calling the
   shared helpers.
4. Differential against StaticCpu, then the Kani and Verus obligations above.
5. A second vCPU only after one vCPU's translation matches the oracle.
6. Measure. Only then decide whether the claim is true.

## iOS floor

The Relay library builds with `IPHONEOS_DEPLOYMENT_TARGET=13.0` and the
latest installed iPhoneOS SDK. A lower deployment setting does not prove API
availability. App Store review is separate from this architecture.

# Relay Mode A completion plan

Audit: 2026-09-30. Scope: the requested iOS/iPadOS App Store StaticCpu Linux
VM and OCI-in-VM path, with both 4 KiB and 16 KiB guests. This is not a claim
that every Relay backend, every ARM extension, or every Wawona platform is done.
Existing mandatory WASI bundles and other product targets must keep working.

## Completeness scorecard (2026-10-06)

iOS / iPadOS VM capability stays **planned**. Completely complete means every
row in `static-cpu-acceptance.md` plus a store-shaped IPA that boots NixOS on
device. That bar is **0%**. Simulator `E4A1C0DE` is not STARDUST and is not
App Store processing.

Judgment only. Harness counts, live seconds, and Kani SUCCESS are not this
percentage. Whole-VM mathematical proof does not exist. Kani 0.68.0 plus Verus
0.2026.09.27.3cf1832 cover named helpers (37 harnesses, `FORMAL_OK` on digest
`0e20b45b…`). Miri is UB, not math.

| Number | Meaning | Value |
| --- | --- | --- |
| Completely complete (iOS Mode A Linux VM + OCI, store) | All 12 acceptance gates and ASC/device | **0%** |
| Twelve-gate progress | Unweighted mean of the table below | **~59%** |
| Six-gap campaign | `relay_acceptance_gaps` items | **~85%** of those six, not of the product |
| Formal coverage of the VM | Named helpers vs interpreter + devices + Linux | **low single digits** |

Proof machine for the six-gap work: `E4A1C0DE` only. Guest `guest-4k-auth10`.
Live log: `Wawona/.artifacts/relay-build/logs/guest-4k-auth17-smoke.log`.

### Twelve-gate table (iOS Mode A StaticCpu)

Weights are equal. Partial is judgment from the listed evidence.

| Gate | % | Evidence | Still reject |
| --- | --- | --- | --- |
| CPU memory semantics | 70 | Native SIMD splits, AF, selected walks | Full advertised ISA / descriptor legality |
| Trace integrity | 60 | JSONL hashes and self-replay | Full-system reference adapter |
| Linux PID 1 | 100 | systemd as init on 4 KiB and 16 KiB | none for this gate |
| NixOS Stage 2 | 100 | Activation then real PID 1 | none for this gate |
| systemd target | 100 | `Reached target Multi-User System` on both granules historically; 4 KiB auth17 at guest 585.79s | failed required units on a fresh 16 KiB image |
| Automatic login | 80 | `wawona` session on recent 4 KiB boots | Host-driven login; 16 KiB parity this week |
| Readiness | 55 | `WAWONA_RELAY_AUTH_OK=1` on vsock 1025 (`WWN1`) plus `WAWONA_RELAY_READY=1` on auth10-17 | Console READY alone; missing verified artifact-hash binding in the receipt |
| Wayland | 85 | auth18: `Relay imported SHM frame: 800x600 sha256=5b6add91…7379c3` after OPEN_FILE 15 MiB + BUFFER_DIFF | iland present in app UI; 16 KiB parity |
| Interaction | 0 | none | keyboard, Multi-Touch, resize, clipboard, audio, reconnect, shutdown |
| Page-size parity | 45 | Multi-User on 4 KiB and 16 KiB earlier; 4 KiB has AUTH + DNS this week | Same AUTH + imported frame + nix run on 16 KiB |
| OCI lifecycle | 15 | Digest/extract tests, confinement regressions | Guest crun create/start/stop in StaticCpu |
| Distribution | 0 | iOS 13 / SDK 26.5 link and Simulator smoke | Signed STARDUST run, TestFlight, ASC |

Mean of the twelve percentages: **~59%**. Round to **55-60%** in chat. Do not
quote the six-gap campaign percent as product complete.

### Six-gap campaign (cursor plan)

| Item | Status | Proof |
| --- | --- | --- |
| link-app | done | `wwn_waypipe_client_fd` on the iOS sim archive; iPadOS `WWNRelay.m` compiles |
| multi-user | done | 4 KiB and 16 KiB Multi-User + `WAWONA_RELAY_READY=1` with journal forward |
| disk-lifecycle | done | Marker, stop, grow 8 GiB, restart, marker back on `E4A1C0DE` |
| nixpkgs-dns | partial | virtio-net NAT; `wawona-fastfetch: dns ok` (`channels.nixos.org` → 151.101.21.91). `nix run` dies TLS EOF on `api.github.com`. No `wawona-fastfetch: end` |
| vsock-frame | done (4 KiB) | auth18 imported 800x600 SHM frame. READY=1. Keep Work Status until app shows it |
| aot-measure | blocked on frame | 3-insn oracle match; two host threads; SharedRam after join; `nixos_translated=0`. No boot profile, no NixOS translate, no speed claim |

### Shortest calendar path (required)

Goal is minimum wall-clock to the remaining acceptance gates, not a faster
guest and not a speed claim. One live StaticCpu. One proof disk. Fix the
narrowest host bug that unblocks the next gate. Do not widen the ISA, rebuild
the guest, or open a second product surface until the current gate is green.

Critical path (serial). Everything else waits or rides the same boot:

```text
live 4 KiB auth17
  -> host SHM import (take_frame + sha256)     [now]
  -> same-boot nix run print if TLS already patched, else next smoke batches TLS
  -> one replacement smoke only if the importer binary must change
  -> 16 KiB same binary, same importer (parity, not a new design)
  -> measure that boot, offline AOT of that closure
  -> one Multi-Touch + Stop on E4A1C0DE
  -> one OCI create/start/stop in the same engine
  -> crate2nix + STARDUST + ASC last
```

P0, this week, do in this order:

1. **Imported SHM frame (only P0).** BUFFER_DIFF already arrives. Do not rebuild
   NixOS, weston, or waypipe. Do not add GPU. Capture one live WMSG, write a
   failing `shm_import` unit test, make `take_frame` log
   `Relay imported SHM frame … sha256=`. Touch only importer + its Kani/Verus
   helper so the formal restamp stays small. Kill and relaunch `static_smoke`
   only after that stamp. Keep `ProjectStatusView` until that log line exists.
2. **nix run print on the same guest.** DNS is done. Do not change `relay.nix`
   flake URLs until host TLS is proved. If auth17 still hits `api.github.com`
   EOF, the next smoke (the SHM binary) is the TLS retry. Exit:
   `wawona-fastfetch: end` plus a real fastfetch print. Not a host DNS test.
3. **16 KiB only after 4 KiB has a frame hash.** Same host importer. No second
   protocol. No 16 KiB guest rebuild unless that boot dies on a new opcode.

P1, only after P0 frame hash:

4. **AOT measure of that closure.** Profile the proven boot. Translate offline.
   StaticCpu stays oracle and fallback. Two host threads already exist. Record
   cold Multi-User, first frame, translator time, signed text, fallback bytes.
   Translator never in the store IPA. No fastest/cleanest sentence.
5. **Thin interaction.** One Multi-Touch tap that the guest sees, then Stop that
   returns to Machines. Defer clipboard, audio, resize, reconnect until that
   pair is green.
6. **Thin OCI.** One verified bundle create/start/stop/kill on StaticCpu. Defer
   publisher trust, virtiofs, and snapshot schema.

P2, last (does not shorten P0):

7. crate2nix for the mobile staticlib. Signed STARDUST. Store IPA audit
   (jitless, no `MAP_JIT`, no HV). ASC/TestFlight as their own gates.

Hard time-savers (do not schedule):

- Second live `static_smoke` while one guest is up
- Guest image rebuild to "help" SHM while BUFFER_DIFF is already on the wire
- 16 KiB, AOT of NixOS, ISA catalog, full-system reference, GIC completeness,
  virtio-rng, virtiofs, SwiftUI, crate2nix, STARDUST, or ASC before the 4 KiB
  frame hash
- Formal restamp caused by editing `aot.rs` / `net_host.rs` during an importer-only
  fix
- Decoder work unless the live guest stops on an unimplemented instruction
- Mode B `IosHv` on this path. Mode A iOS stays StaticCpu / Pulley

Mode A iOS stays StaticCpu / Pulley. Formal stamp before Relay VM cargo.

## Historical ordered work map (2026-10-01)

Rough total engineering estimate then: 50% (45-55% range). Superseded as the
headline number by the 2026-10-06 scorecard above. Historical checkpoints below
retain dates.

| Order | Current evidence | Remaining acceptance |
| --- | --- | --- |
| 1. Host and iOS build | 316 host tests; full FMUL/SHM/exit Simulator phone/watch app, both guest images and scoped gates pass; prior device iOS13/SDK26.5 gates still pass | Final app with new bounded Stop, runtime and distribution checks |
| 2. RAM/storage lifecycle | Physical QA create/save/reopen retains768MiB/9GiB; device disk metadata confirms9GiB; exclusive disk/growth tests | Actual guest boot/stop/grow/restart and guest data preservation |
| 3. Guest services | Earlier 900s fused-fix guests reach Multi-User and user manager; fresh FMUL/SHM images remain live900s | Fresh required target, measured hvc0/coldplug/DHCP failures, optional OCI mount, BPF difference, required Wayland/frame |
| 4. Transport/graphics | Both guests send real vsock bytes; real native host waypipe FD entry is linked, reconnect/stop unit tests pass | Actual native session, authenticated readiness, genuine imported guest frame; VZ direction/readiness repair |
| 5. Devices/OCI | Block, console, vsock wired | rng/net/fs, OCI lifecycle, real input/resize/clipboard/audio/reconnect/shutdown |
| 6. Verification | 26 Kani, 15 Verus; full36 strict Miri suites, prior452,538 ASan fuzz runs | Full-system differential reference and real native-worker concurrency validation |
| 7. Performance/AOT | Offline signed image, software TLB, invalidation, and one host thread per vCPU call `execute_fetched`. Translator is `aot-translate`, not the store staticlib. A 3-instruction block matches StaticCpu with zero fallback. Two vCPUs run on two host threads. NixOS image is not translated. No speed claim. | Profile a real boot, translate that closure, shared RAM, then measure. |
| 8. Physical/distribution | Current exact phone/watch signing and strict bundle gates pass | Exact signed app installed and launches on unlocked STARDUST; guest runtime and distribution export/validation |

Current physical UI finding: the actual native VM editor exposed only Backend,
while shared UI already had RAM/disk controls. Native profile-schema controls
now pass full phone/watch linking, signing, install and physical save/reopen.
QA profile FE02CE81-DB08-4508-97C4-6C06381989C1 retains768MiB/9GiB.
Physical start is a live black surface, with no accepted guest frame; effective
input mode remains unverified. Store app had no persisted boot evidence.
New bounded Rust console.log records console/elapsed/PC/instruction samples,
with309 tests/26 Kani/15 Verus/full33 strict Miri passing. Fresh full phone/watch
app builds/signs/installs and passes signed graphics/ModeA floor gates. Physical
4KiB trace confirms configured768MiB, writable ext4 root, /wawona-init PID1 and
NixOS Stage2 and Multi-User System. At1185760ms/11097739264instructions it
stops on indexed FMUL V29.2D,V29.2D,V26.D[0] (0x4fda93bd). Indexed S/D
forms reuse existing unsigned-lane/software-FP helpers;9216 fixed-native
comparisons,312 host tests,26 Kani and15 Verus pass.35 strict Miri suites pass.
Earlier guest waypipe reports Vulkan instance creation failure (no guest driver),
then Cage cannot select a swapchain format. StaticCpu mobile guest recipes now
negotiate --no-gpu SHM on the real waypipe channel; new-image runtime unverified.
Native exit detection omitted Relay ownership before any frame. Existing-handle
queries now enable the host exit path and retain machine identity on pending stop;
updated full-app runtime verification remains pending.
User requested iOS Simulator: baseline installed app opens through agent-device,
but is older than current sources. Simulator/iPad package arguments omitted
both guest-image paths, so even a complete link would omit required VM data.
Phone Simulator and iPad Simulator/device recipes now supply both artifacts.
Full current Simulator product builds with real Weston/Niri/Relay via
build-current-ios.nix --arg simulator true. Bundle/image/runtime checks pending.
Authenticated frame and graceful guest stop remain unverified.
Native waypipe links and executes the upstream handler with a duplicated real
vsock FD. This is launch plumbing, not authenticated readiness or a frame.

## 1. Decision and measured baseline

Continue from the existing Rust implementation. First complete the independent
reference and verification boundary, then repair the CPU and device contracts
against it. Do not make successive multi-minute boots the instruction catalog.

Current local gates pass: 281 workspace tests (225 VM tests), 22 Kani
harnesses and 11 Verus checks. All 26 strict-provenance Miri suites pass. The iOS 11
release-library build, shared WawonaUI package and Clippy checks pass. Console
queues now retain independent notifications, preflight descriptor payloads,
use fixed scratch space and avoid empty receive completions.
These are scoped verification/build results, not coverage percentages or
physical-device/distribution evidence.

Both direct-root page sizes finish NixOS Stage 2 and run real systemd PID 1,
generators and service startup. The SRCU wait was caused by discarded self-SGIs
and is repaired. Measured reductions, table lookup, scalar ADDP and saturating
arithmetic now have shared implementations and native comparisons. Interleaved
transfers add 126 native forms; high-lane FMOV has native and portable checks.
Both granules subsequently remained live for 480 wall-clock seconds without
an unsupported-instruction stop. Required target readiness remains unverified.
The 16 KiB firewall kernel recipe now enables its Netfilter dependencies;
full build passed; boot validation remains pending. A longer 4 KiB run exposed
UMAX (now repaired), then vector CLZ (now repaired and verified), and an nsncd
SIGSEGV at address -48. Its captured context identified discarded logical-
immediate SP writes during stack realignment; the reduced frame regression
passes after correction. A full run now starts nsncd cleanly and stops later
at FCCMP. Conditional FP decoding and persistent-storage changes are under
fresh validation: 281 workspace tests, 22 Kani and 11 Verus checks pass.
The shared UI package and iOS release library build; all 26 Miri suites pass. Both refreshed guests remained live for 600 seconds; service timeouts
still prevent required-target acceptance.

Two initial boot milestones are evidenced (Linux PID 1 and Stage 2 completion).
Seven remain; this unweighted milestone count is not engineering completion.
Earlier checkpoints below record historical snapshots and their then-open gaps.

## 2. Confirmed gaps and risks

| Area | Audit finding | Consequence |
| --- | --- | --- |
| Independent reference | Trace writer/comparator and 58 fixed EL1 hardware observations exist; full-system reference adapter does not | Wrong semantics can survive until a later boot symptom |
| CPU feature contract | Many instruction families have native tests; no complete coverage inventory for the advertised CPU profile | Missing and overbroad decoders remain an unknown-sized task |
| Instruction fetch/MMU | Fetch/read/write permissions, AF and selected descriptor legality now have tests/reference evidence | Complete descriptor/canonical-address legality, system-register privilege checks and the full advertised CPU profile remain open |
| Interrupt controller | Single-CPU SGIR routing now works; other distributor writes and enable/target reads remain simplified | Masking, priority, active/pending state and level reassertion need an architectural contract |
| Devices | Static bus wires block, console and virtio-vsock, not rng/net/fs | Guest service configuration cannot substitute for the missing devices |
| Vsock | Real guest queues/credit and total budgets are now wired to actual Unix streams | Native host waypipe, authentication, imported frames and full lifecycle acceptance remain |
| Storage | App-owned per-machine files now retain writes, hold advisory locks, enforce 4..64 GiB growth limits and flush write completions | Crash recovery, external filesystem races and snapshot coherence remain unproved |
| OCI | Digest/diff-id validation and bundle materialization exist | Publisher trust, immutable validated handles, bounded decompression and safe publication remain |
| OCI confinement | Archive-created symlink ancestors reject; tar extraction confines hardlinks; whiteouts run before additions | Concurrent ancestor replacement, reopened validated blobs and atomic publication remain open; internal symlink ancestor resolution is conservatively rejected |
| OCI identity | OCI `User` and explicit numeric UID:GID are preserved; malformed, named and UID-only identities reject explicitly | Confined account lookup remains needed for named/UID-only images |
| Formal coverage | Small production helpers and separate mathematical lemmas | No end-to-end CPU, MMU, queue, lifecycle or model-to-implementation proof |
| Proof register | Updated helper-by-helper obligations distinguish native, Kani and Verus evidence | No end-to-end refinement or coverage percentage follows from harness counts |
| CI coverage | Native asm tests are excluded on non-AArch64; current workflow uses Ubuntu x86 runners | A green ordinary CI run does not establish native ARM conformance |
| Miri/fuzz/concurrency | Miri now gates selected MMU, device, SIMD, FP and hardware-reference paths; fuzzing and concurrency remain limited | Executed-path checks do not establish whole-boundary or lifecycle coverage |
| Release evidence | Local source stamp covers VM/spec sources, not full build/toolchain/guest provenance | Local runner gate is not a signed runtime launch attestation |
| Packaging | Mobile staticlib still uses a monolithic buildRustPackage recipe | Migrate to the required crate2nix path and validate the actual linked app |

The OCI confinement rows identify assurance gaps from code inspection, not a
claimed reproduced exploit. The measured SRCU wait is repaired on both page sizes.
Later service failures and instruction gaps still prevent target acceptance;
these need their own diagnostics rather than attribution to the repaired wait.

## 3. Slim-runtime constraints

Keep one production implementation per mechanism. Reuse `GuestMemory`, checked
guest spans, the virtio transport and the same Linux VM for OCI. Prefer small
enums, structs and pure helpers over a plugin framework or a second runtime.
Keep platform glue at the ABI boundary. Use the existing pinned software FP
dependency unless measurements or correctness evidence require a change.

Reference emulators, trace tools, corpora, fuzzers, Kani, Verus and benchmark
tools stay outside the app dependency graph. Diagnostic tracing should be a
development feature, not unconditional runtime overhead. Test code may be large;
shipped code and its trusted boundary should stay small.

Measure linked Relay text/data, compressed app contribution, bundled guest size,
peak/resident RAM, boot time and frame latency separately. An archive's file
size is not the final linked app size. Establish budgets from real baseline
measurements and check deltas. Stream bounded inputs; avoid full rootfs copies.
Measure size-oriented optimization/LTO settings locally before changing release
defaults. Never remove required semantics, error checks or target bundles for size.

Start with the existing single-vCPU platform and a precisely declared AArch64
feature profile. Implement mandatory instructions of that profile. Optional
extensions may be absent only when feature registers truthfully say so.
Do not mistake an observed boot opcode list for architecture completeness.

## 4. Divide-and-conquer tracks

These are implementation ownership boundaries, not a request to create separate
engines. Work can proceed independently after shared contracts are fixed.

### A. Evidence and independent execution

1. Create a compact requirements/ISA inventory linking each requirement to its
   implementation, reference test, proof obligation and acceptance evidence.
2. Version the execution-event contract: instruction completion, synchronous
   exception entry, external interrupt, MMIO result and nondeterministic input.
   Current interpreter steps are not architectural retired instructions.
3. Add a host-only reference adapter. Use fixed native ARM instructions for
   EL0 semantics, privileged micro-guests for MMU/exception/device boundaries,
   then an external full-system reference with matching platform configuration.
4. Match initial RAM, registers, images, DTB and disk overlay. Record/replay
   timer events, device responses, network input and entropy. Production entropy
   stays secure; deterministic test replay must never become production RNG.
5. Validate one deliberately injected CPU error, one MMU error and one device
   error are located and minimized. Require an expected endpoint so a matching
   truncated prefix cannot count as a complete run.

**Exit:** independent traces reproduce agreed micro-guests and locate known
mutations at the first architectural divergence. Whole-boot comparisons begin
only after event alignment works. Shared host services are an explicit trust
assumption; sharing them does not independently verify those services.

### B. CPU, translation and exceptions

1. Audit decoder masks, reserved encodings and advertised feature registers.
   Refactor only the helpers needed for independent checking, not a full rewrite.
2. Complete integer, load/store, exclusive, FP/SIMD and system instruction
   families in the declared profile. Include flags, aliases, zero/SP registers,
   boundary shifts, NaNs, rounding and exception behavior.
3. Make memory access kind explicit: instruction fetch, data read, data write.
   Validate privilege, permissions, table descriptors, address size, granule,
   alignment and architectural faults at every applicable level.
4. Test exception entry/return and exclusive monitor transitions. Check barriers
   and future cache invalidation against the single-vCPU execution model.
5. Test cross-page accesses and fault-visible partial effects according to each
   instruction's architecture contract. Do not generalize a helper's atomicity
   policy into an unsupported claim about all ARM instructions.

**Exit:** each declared family has independent valid/reserved tests; privileged
micro-guests agree with the reference for both page sizes; no known mismatch.
Report finite/random test coverage separately from universally proved helpers.

### C. Minimal VM platform and real boot

1. Correct GIC/timer enable, mask, priority, pending/active/EOI and level-trigger
   behavior. Remove duplicate test-only device models or explicitly connect them
   to production so passing their tests cannot hide a different runtime path.
2. Harden shared virtqueue validation once: bounded acyclic descriptors,
   direction, checked spans, index wrap, negotiated features, reset and used
   lengths. Add small device handlers over that transport.
3. Implement virtio-rng, then virtio-vsock. Complete virtio-net and bounded
   host userspace networking using public socket APIs for release networking.
   Network setup must not become an unnecessary dependency of first readiness.
4. Diagnose the existing 16 KiB wait with matching kernel symbols, event traces
   and service states. Repair guest early mounts/root/initrd configuration;
   eliminate current `/proc/cmdline` and `/dev/fd` errors in the 4 KiB path.
5. Implement guest-triggered reset/shutdown and reliable cancellation. Validate
   the exact release command line after diagnostic boot succeeds.

**Exit:** real PID 1 is systemd, intended target active, required units healthy,
automatic intended-user login verified, repeated on both guest bundles.

### D. Vsock, Wayland and app lifecycle

1. Implement connection state, credits, aggregate byte/connection limits,
   disconnect and reset in the real virtio-vsock transport.
2. Add one small versioned guest-control protocol for readiness and lifecycle.
   Bind a fresh challenge, machine/session ID, guest hashes and reported unit
   state to an authenticated response. Do not treat a bundled static secret or
   console marker as attestation. Define key provisioning and guest-agent trust;
   this is session authentication, not hardware attestation of arbitrary guests.
3. Connect existing waypipe to that transport and Wawona's compositor. First
   establish a bounded CPU/shared-memory frame path; profile before zero-copy.
4. Prove interaction by observing guest results: keyboard, multi-touch, resize,
   clipboard, audio, reconnect and shutdown. Add app background/resume and
   cancellation interleavings.
5. Design versioned snapshots covering CPU/MMU state, exclusive state, devices,
   virtual clock, pending inputs and disk overlay generation. Quiesce I/O;
   reject mismatched images/schema. Reset and reauthenticate live external
   connections on restore. Fast startup snapshots are a later optimization.

**Exit:** fresh authenticated readiness, real guest-derived frame and successful
UI interactions on both page sizes; repeated lifecycle/snapshot tests show no
stale sessions, leaked handles or lost committed disk state.

### E. OCI, storage and confinement

1. Define publisher trust separately from digest integrity. Verify signatures
   under that policy using an established library, not custom cryptography.
2. Add explicit limits for compressed/uncompressed bytes, files, paths, nesting
   and aggregate disk usage. Stream decompression and hashing.
3. Close validation-to-use gaps using immutable verified blobs/handles and
   atomic publication. Use capability-relative/no-follow traversal, with
   symlink, hardlink and whiteout semantics tested across multiple layers.
4. Preserve OCI command, identity, environment and working-directory semantics.
   Never silently substitute a readiness shell or root identity in production.
5. Complete a minimal in-process virtiofs backend over the existing transport
   for verified bundles; no mobile host daemon/fork. Keep immutable root storage
   plus a quota-bounded persistent writable overlay and defined flush semantics.
6. Run the bundled guest crun through the same VM/control/Wayland path. Exercise
   create/start/stop/kill/restart, exit codes, interrupted extraction, full disk,
   corrupt layers and recovery.

**Exit:** real OCI images pass lifecycle and confinement tests inside StaticCpu;
base content remains immutable and acknowledged persistent writes survive restart.

### F. Verification and release integration

Begin alongside A, not after the runtime is finished.

1. Expand the obligation register, production-code pairing and automated test
   matrix. Pin verifier/compiler versions and hashes. Add ARM64 native CI;
   make skipped mandatory corpus families visible failures for release evidence.
2. Extend the local source gate to relevant core/OCI/FFI code, build features,
   scripts/configuration and tool identities. A local stamp remains a developer
   convenience, not a tamper-proof security boundary.
3. Run proofs, tests, differential corpus, Miri, Loom, fuzzing and build checks
   for the same source/configuration. Store logs, bounds, assumptions and seeds.
4. Produce a signed evidence manifest mapping requirements to results and
   binding source, toolchain, dependencies, guest hashes and runtime build ID.
   Use an embedded payload manifest plus an external attestation of the final
   signed IPA to avoid circular self-hashes. Reject absent, mismatched, stale
   or untrusted required evidence at the release launch boundary.
5. Migrate mobile Relay recipes to crate2nix. Prove local links and bundle
   contents before CI. Verify no proof/reference/JIT/private-API dependencies
   leak into the Mode A app.
6. Run clean-install Simulator UI acceptance, then physical iPhone/iPad testing
   of the exact release build. Capture logs, readiness, frames, memory and timing.
   Finish archive/export and TestFlight/App Store evidence as separate external
   gates; review outcomes are not mathematically guaranteed by runtime tests.

**Exit:** release evidence is complete and tied to the shipped artifact, all
mandatory acceptance cases pass, and residual trust assumptions are explicit.

## 5. Mathematical proof strategy

For a subsystem transition `T`, invariant `I`, and admissible input predicate
`P`, prove: `I(s) and P(input) implies I(T(s,input))`, together with the required
functional result. For rejected inputs, specify which state may change and
prove that exact error contract. Then prove callers establish callee preconditions.
For architecture conformance, implementation steps must correspond to allowed
specification transitions, including faults and permitted nondeterminism.

| Mechanism | Kani on production code | Verus | Additional evidence |
| --- | --- | --- | --- |
| Decoder/ALU | Masks, widths, flags, shifts, overflow and rejection | Selected semantic/refinement lemmas | Native ARM corpus; instruction-sequence differential tests |
| Memory/MMU | Span arithmetic, descriptor decoding and finite walkers | Region separation, translation/permission invariants | Privileged reference micro-guests; Miri on Rust memory boundary |
| Virtqueues/devices | Descriptor and ring transitions within declared bounds | Ownership, index/credit conservation, reset invariants | Adversarial device tests, fuzzing, real Linux drivers |
| Storage/OCI | Bounded path components, counters and state transitions | Confinement model and publication/lifecycle invariants | Real filesystem/symlink tests, decompression fuzzing, crash injection |
| FFI/lifecycle | Valid-contract pointer/length and handle-state helpers | Ownership/state-machine rules | Miri for supported Rust paths, sanitizers/native ABI tests, Loom |
| Floating point | Tractable bit-level helper properties | Explicit selected conversion/rounding properties | Independent FP differential corpus; dependency remains outside current proof |
| Release gate | Manifest parsing/selection and failure logic | Evidence-binding state invariants where useful | Tamper, wrong-key, downgrade, stale-build and artifact-replacement tests |

Kani proofs must state all input bounds and assumptions and pass unwinding
assertions. Add reachability witnesses so contradictory assumptions do not make
a proof vacuous. A small bounded parser harness does not prove arbitrary files.

Verus currently proves separate models. Prefer verifying the same small pure
production kernel where practical; otherwise prove a correspondence layer or
mark it as reviewed, unproved trust. Named functions alone do not establish
refinement. Inventory every axiom, `assume`, `external_body` and external spec.
Never weaken requirements or exclude real inputs merely to get a green result.

Miri checks executed Rust paths for undefined behavior; it is not mathematical
proof or an ARM CPU oracle. Native asm and external framework/ABI behavior need
separate native tests. Loom explores modeled concurrency schedules; specify
its bounds and use the same production synchronization abstraction. Fuzzing and
mutation tests exercise malformed input and verify the tests can detect bugs.

Safety does not imply liveness. Prove local progress with explicit fairness and
resource assumptions where feasible; measure whole-guest boot, shutdown and
service progress with deadlines and reproducible failure diagnostics.

References: [Kani bounds](https://model-checking.github.io/kani/tutorial-loop-unwinding.html),
[Verus trust boundary](https://verus-lang.github.io/verus/guide/tcb.html),
[Verus proof guardrails](https://verus-lang.github.io/verus/guide/llmforverusproof.html),
[Miri](https://github.com/rust-lang/miri), [Loom](https://github.com/tokio-rs/loom).

## 6. Dependency order and completion accounting

```text
Freeze contracts and slim-runtime budgets
  A: independent reference ----> B: CPU/MMU --------+
  C: platform/devices -----------------------------+--> real systemd
  E: OCI/storage hardening ------------------------|         |
  F: evidence/CI/build integration ----------------|         v
                                                  +--> D: readiness/Wayland/UI
                                                            |
                                              E: OCI in the same running VM
                                                            |
                                              device + release acceptance
                                                            |
                                              profile and optimize, reverify
```

A and B share CPU files; coordinate changes instead of parallel edits to the
same decoder. C owns bus/device files; E owns OCI/storage; F owns evidence/build
integration. All tracks use the same memory, virtqueue and error contracts.
Small reviewable changes should each carry tests and their proof delta.

Next implementation batch:

1. Repair evidence accounting and add ARM64 CI plus expanded Miri targets.
2. Establish the reference event contract with one CPU/MMU/device micro-guest.
3. Write failing instruction-fetch privilege/execute tests and repair that path.
4. Add a reference-backed complete family for the latest unsupported opcode.
5. Diagnose the 16 KiB wait while device/OCI negative tests advance independently.

There is substantial platform work left, not merely several opcodes. CPU/profile
closure and the reference adapter are the largest unmeasured risks. A credible
calendar estimate needs the ISA inventory, adapter feasibility result, a real
systemd run and physical-device memory/timing baselines. Do not invent an ETA
from 177 tests or extrapolate elapsed boot steps into feature completion.

Report separate counters: boot milestones, required feature contracts, proved
properties by scope, reference-covered ISA families, and release matrix cells.
Freeze denominators before reporting percentages; expanding scope must stay
visible. A property is not complete just because some harness in its module passes.

100% means every agreed release requirement has current passing evidence, every
required mathematical claim is discharged within stated assumptions, no known
required failure remains, and the exact app artifact passes its release gates.
It does not mean proof of Linux, LLVM, UIKit, hardware or absence of all future bugs.

## 7. Implementation checkpoint after this audit

The first Track B fix now uses explicit fetch/read/write accesses through one
shared production walker. Fetch checks the executing privilege, leaf and table
execute-never, table AP restrictions, WXN and the EL1 prohibition on executing
EL0-writable mappings. Typed walk failures preserve the actual fault level;
unsupported configuration errors no longer become fabricated instruction faults.
Instruction/data abort entry shares one implementation, including EL1 SP0 context.

Three new fetch regressions failed on the previous implementation. Those plus
three additional permission/fault regressions pass on 4/16 KiB fixtures. The
suite now reports 183 VM tests, 3 smoke tests, 13 Kani harnesses and 9
Verus-reported checks. These prove the named scope only; AF, canonical address
validation, complete descriptor legality and privileged reference micro-guests
are still open. The full-system reference adapter is still missing.

Track F now has an ARM64 native-corpus CI job with a corpus-presence check, an
expanded Miri script, a pinned CI Miri nightly and an updated proof register.
The workflow configuration has not yet been exercised on GitHub.

### Further measured implementation

The AF gap named above is now repaired for the advertised software-managed
profile, with read/write/fetch fault regressions. Descriptor legality,
canonical address checking and privileged reference equivalence remain open.
Widening integer add/subtract, consecutive LD1/ST1 structures and immediate
shift families use shared implementations and fixed native instruction corpora.

Both rebuilt 4 KiB and 16 KiB direct-root guests start the real systemd PID 1.
This replaces the earlier 16 KiB initrd-udev stall as the current baseline;
required systemd target readiness is still unverified. Do not raise the
release-completion percentage from instruction counts or this partial boot.

### Hardware-reference checkpoint

A standalone macOS development oracle now records 29 fixed EL1 micro-guests
on both page sizes (58 observations). It is outside the product and compares
hardware exception state, stack banks, memory and timer status against StaticCpu.
It exposed and drove fixes for SVC ESR.IL, EL1 SP0 exception vectors, IRQ masks,
invalid level-3 descriptors, ERET stack selection and masked timer ISTATUS.
All 202 VM tests pass after these fixes. This bounded reference does not replace
the still-missing full-system Linux trace adapter or establish ISA completeness.

The debug 4 KiB boot located the post-hostname stall: systemd closes an inotify
instance and waits in fsnotify_wait_marks_destroyed; the cleanup worker waits in
synchronize_srcu. This is a measured blocked-task stack, not yet a diagnosed CPU
or timer defect. GIC level/active/mask state and timer delivery remain priorities.

Latest validation: 252 workspace tests, 14 Kani harnesses, 9 Verus checks,
14 strict-provenance Miri suites, fresh 58-observation hardware comparison,
Clippy correctness/suspicious checks and the iOS 11-targeted release static-library
build passed. Both fresh 300-second guest runs reach systemd/hostname but do not
reach readiness. The fixes above do not resolve that wait.

Next diagnostic must distinguish SRCU reader-counter imbalance from a missing
scheduled callback using the exact kernel symbols and state. The 4 KiB kernel
is `/nix/store/876bng7sgzbwnbn8xwhgrfk15dsv3qlr-linux-7.2.3`; its source archive is
`/nix/store/qnp0zhpgbabb9m9c900cai61b3hwfxn8-linux-7.2.3.tar.xz`.
Relevant symbols include process_srcu, srcu_irq_work, srcu_reschedule and
srcu_invoke_callbacks. GIC IAR=1023 counts alone do not diagnose lost IRQs:
Linux can drain an interrupt and then read the normal spurious terminator.

### Software-interrupt gap

A feature-gated kernel probe recorded SRCU grace-period requests without a
subsequent `srcu_irq_work` entry in a three-billion-instruction baseline. The
exact guest kernel's single-CPU `gic_ipi_send_mask` path writes GICD_SGIR with
TargetListFilter=2; the previous Bus discarded all distributor writes. SRCU uses
irq_work to schedule its grace-period worker, so this missing path can strand
synchronization even with balanced reader counts.

The bus now routes explicit CPU-0 targets and self-targeted SGIs, leaving
other-CPU-only and reserved filters undelivered. Kani checks all input bits;
Verus models the single-CPU routing contract. This does not close the wider GIC
mask, priority and active-state gaps. The register contract comes from
[Arm IHI 0048B, section 4.3.15](https://documentation-service.arm.com/static/5f8ff21df86e16515cdbfafe).

The SGIR fix resumed `srcu_irq_work`, `process_srcu`, and callback completion.
Both 4 KiB and 16 KiB guests passed the old wait and reached a UMAXV instruction
in systemd startup. Shared integer reductions now cover 35 native forms:
ADDV, signed/unsigned widening add, min and max. Kani checks exact sums modulo
result width and min/max membership/order over every valid kernel input.

Both guests next stopped at in-place TBL V31.16B,{V31.16B},V28.16B. Shared TBL/TBX
now covers 16 native forms, with wrapping table registers, destination overlap,
index boundaries and Q0 upper clearing. Kani checks exact byte selection for
arbitrary vectors and counts. These replace individual decoder misses with
shared families; neither establishes full ISA or guest-readiness completion.

## Per-machine iOS VM settings (required)

Machine type Virtual Machine must expose a storage-capacity slider and other
supported VM options directly in Wawona iOS per-machine settings. Persist values
per machine and wire them into actual Relay allocation/start behavior. Show
valid ranges, preserve machine data and make resize/restart requirements clear.
Controls without backend effects do not satisfy acceptance. Reuse the existing
profile model and keep validation/product policy in Rust.

Implementation order for the settings requirement:

1. Share Rust allocation/resize policy between RelayMachineProfile and RelaySpec;
   honor RAM and disk settings after checking the original guest trust record.
2. Add durable per-machine writable disks to StaticCpu, including exclusive
   ownership, growth without truncation, quota/error handling and restart tests.
3. Wire existing vmSettings through the SwiftUI machine draft and iOS legacy
   editor. Storage slider, RAM and packaged guest choice must affect startup.
4. Verify create/edit/reopen/start/stop/grow/restart from the actual iOS UI.
   CPU count must reflect supported backend capabilities, not expose fake SMP.

## Per-machine disk growth

`storage::capacity` is paired with Kani
`disk_capacity_preserves_existing_data_and_quota`: accepted capacities cover
both the base and existing extent, remain within 4..64 GiB and quota, and are
GiB aligned. Verus `disk_growth_preserves_extent` proves a separate arithmetic
model. Neither establishes filesystem crash consistency, exclusive ownership
against noncooperating writers, or a model-to-filesystem refinement.

Runtime regressions exercise lock contention, rejected shrink, persisted queue
writes, explicit virtio flush, zero-filled growth and reopen. Conditional FP
comparisons additionally use all 16 native conditions and quiet/signaling NaNs;
these are conformance samples, not a complete CPU proof.


## Scalar fused arithmetic and service traces (2026-10-01)

Fresh 16 KiB boot failed after 9,859,432,448 interpreter steps at 0x1f4e0000
(FMADD D0,D0,D14,D0). A failing aliased-source regression preceded repair.
FMADD/FMSUB/FNMADD/FNMSUB S/D now share rustc_apfloat fused multiply-add
with the existing software FP kernel: single rounding, input sign changes,
addend-first NaN priority, FZ/DN and sticky FPSR. No host FP state or runtime
dependency was added. Eight fixed native forms compare 812,032 cases across
RMode/FZ/DN, signs, zeros, subnormals, NaNs, infinity and random operands.
Portable cancellation tests distinguish fused from multiply-then-add results.
Miri passes the shared FP helper and exact guest instruction regression. Use
explicit GuestMemory::allocate_on_host in portable CPU fixtures; macOS
getpagesize is outside Miri's foreign-function support.

Fresh complete workspace/all-targets run: 287 tests, including imported WPM
tests; this expanded denominator must not be confused with release progress.
Existing 22 Kani and 11 Verus checks pass; these do not prove full floating
arithmetic or the whole VM. Miri's gate now includes the fused CPU fixture.
Both original 600-second boots exposed /dev/hvc0 device timeouts. A bounded
4 KiB wait probe confirms SRCU irq_work, workers and callbacks execute, so
do not re-diagnose the old missing-SGIR fault without new evidence. The 4 KiB
guest remained live; the 16 KiB run failed at the new FMADD gap. Required
target, authenticated readiness and real guest frames remain open.


## Virtio-vsock wire/window foundation (2026-10-01)

relay-vm/vsock_wire validates 44-byte LE headers and <=64 KiB payloads without
trusting guest length fields. Preserve unknown type/op values for the required
RST handling in the forthcoming transport. available_credit subtracts modular
outstanding bytes with checked subtraction. Kani production helper and Verus
integer model prove that an accepted window conserves allocated credit; totals
23/12 pass. Fixed UAPI fixture, wrap/invalid-window tests and strict Miri pass.
The old loopback queue is not a guest device; MMIO queues, bounded real host
streams, half-close/reset and authenticated waypipe/frame integration remain
required. Do not call the codec or a console READY hint transport acceptance.
Spec: https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html#x1-32800010

## Virtio-vsock device wiring (2026-10-01)

Both fused-fix 900s traces reach Multi-User and the user manager. Required
Wayland units still fail; hvc0 and optional OCI mount failures remain. Legacy
console READY text is not authenticated readiness. The guest post-start script
now stops on a failed kill check and emits only TRANSPORT_STARTED. Native VZ
console readiness remains an open security/acceptance gap.

StaticCpu now exposes real virtio-vsock device 19 at 0x0a002000, guest CID 3,
three queues and SPI 36/GIC 68. RX/TX use shared split-ring validation, whole
chain preflight and bounded packet allocation. RX kicks survive empty output;
TX progresses within a total budget while RX is absent. The device polls real
nonblocking Unix stream pairs; host CID 2 accepts guest-initiated connections
on port 1024. Product API take_vsock_connection hands off actual host streams.
Status reset drops flows and queued bytes. Flow limit 16, listener limit 8,
per-flow pending window 64 KiB, outgoing limit 256 packets/1 MiB.

Forward-count advancement is validated independently of peer buf_alloc before
modular credit arithmetic. The previous unchecked interpretation of the wire
helper could accept a forged advance with a u32::MAX peer allocation. Kani
checks the actual update helper; the Verus model covers arithmetic only.
Sources reserve peer credit before queuing host bytes; half-close drains guest
payload before closing the host write half. Guest RAM faults cause no partial
RX payload effects. CPU boundary polling prevents host data from depending on
guest MMIO writes.

Direct iOS waypipe --socket-fds still enters unreachable match arms in the
bundled source. Use its real Unix listener path for the host bridge rather
than inventing FD support. No host waypipe client, authenticated readiness or
imported guest frame has been accepted yet. The --vsock-probe smoke option
only records real guest connection/payload evidence. Its output is explicitly
not an authentication or frame result. Stateful fuzz target vsock_packets is
development-only. Fresh checks and boot evidence are under .artifacts/relay-build.

### Fresh scoped evidence
297 workspace/all-targets tests pass (238 VM), 24 Kani harnesses and 13 Verus
checks pass. Seven vsock-related strict-provenance Miri tests pass. The
ASan/libFuzzer stateful packet-sequence target completes 452,538 runs in 46s
with no findings (six initial seeds; capped input length 131,072). This is
a short fuzz campaign, not exhaustive state or concurrency validation.
Clippy correctness/suspicious and workspace formatting pass. The 31 configured
Miri suites have not all been rerun for this checkpoint; only the seven related
tests above were freshly executed.

Fresh 900s 4 KiB and 16 KiB vsock-probe boots are running against prior guest
images. A full current iOS product build refreshes the guest scripts and app.
Neither a guest transport payload, an unauthenticated marker, nor these tests
is a graphics or release acceptance result. Logs live under
Wawona/.artifacts/relay-build/{host-tests-vsock-final,miri-vsock-device,
fuzz-vsock-device,current-build-vsock,guest-4k-vsock,guest-16k-vsock}.log.

### Current app build/signing result
Full current phone/watch build passes: /nix/store/c1baycz9zwmfqr1x1fpwm0yr56ncjm68-Wawona.
Main phone and embedded Model/UIContracts are platform iOS, min13.0/SDK26.5.
Watch fat binaries carry watchOS10.0 arm64_32 and watchOS26.0 arm64 slices.
No newer-iOS link warning is present. Mode A absence and graphics policy gates
pass before and after signing. The isolated signed-device-vsock/Wawona.app
passes strict deep codesign verification. Physical installation was attempted
and rejected before install by DDI mount: kAMDMobileImageMounterDeviceLocked.
STARDUST is paired/available but requires its passcode. No physical runtime
or distribution acceptance is inferred.

Build still warns about duplicate cube-HUD symbols, wpm_main, demo main and
rust_eh_personality across native archives. ANGLE Volk collisions are absent;
these remaining unrelated ownership/toolchain warnings stay mapped for repair.
Diagnostics: deployment-vsock.json, app-build-vsock.log, sign-device-vsock.log,
signed-vsock-{mode-a,graphics}-gate.log, device-install-vsock.{json,log}.

## First real guest vsock bytes and pairwise-long repair (2026-10-01)

The 16 KiB 900s vsock probe establishes a genuine guest CID3 connection to
host CID2 port1024 and reads the first 16 guest waypipe bytes. This is transport
evidence only: the diagnostic does not reply as waypipe, authenticate readiness
or import a frame. The same run stops after 9,985,236,992 instructions at
0x6e202800, disassembled by the Apple toolchain as UADDLP V0.8H,V0.16B.
The 4 KiB run stays live for900s and reaches Multi-User, but guest waypipe
reports ECONNRESET without a host accepted-stream observation. Do not assume
the 16 KiB transport result applies to 4 KiB.

SADDLP/UADDLP/SADALP/UADALP now share pairwise_long_lane. Source widths8/16/32
widen exactly; optional accumulation wraps in twice that width. Snapshot Rn
and prior Rd for aliases; Q0 clears upper64; size3 fails without register/PC
mutation; flags are unchanged. Native comparison executes 48 fixed register
forms (24 arrangements times aliased/separate operands), 70 samples each:3,360
comparisons. The measured alias has a portable regression. Kani checks actual
helper against independent i128 arithmetic; Verus models signed bounds and
modular accumulation. Neither proves the whole decoder/CPU.

Fresh checkpoint: 299 workspace/all-targets tests (240 VM), 25 Kani harnesses,
14 Verus checks pass. Pairwise regression passes strict-provenance Miri. Latest
iOS13/SDK26.5 relay-ffi release library compiles. Previously signed full vsock
app predates this CPU repair; a fresh full-app link is still required.
Bounded first128 header tracing is kernel-probe-only, excluded from default
product. Fresh 4 KiB/16 KiB probes running under this source will diagnose the
reset and verify progress after UADDLP; target/frame acceptance stays open.

## Pairwise full app and 4 KiB module loading (2026-10-01)

Fresh pairwise16KiB probe stays live900s and sends16+28 real waypipe bytes;
4KiB stays live900s but sends no observed virtio-vsock headers. Exact4KiB
kernel config has VSOCKETS/VIRTIO_VSOCKETS=m;16KiB has builtins. Its net-pf-40
alias selects VMCI, while virtio device19 aliases the correct transport.
Direct-root initrd=null skips boot.initrd.availableKernelModules. Guest now
loads vmw_vsock_virtio_transport at sysinit via boot.kernelModules. This is a
measured configuration discrepancy, not yet proof of the ECONNRESET cause.
Fresh4KiB probe uses refreshed current-app-pairwise image and must show traffic.

Full app /nix/store/1cwvnpnncz67bnq5ihz3vs1arsg65mv9-Wawona embeds this guest
change and pairwise CPU repair. Phone/main embedded frameworks have iOS13.0,
SDK26.5; watch arm64_32 floor10.0 and arm64 floor26.0 both use SDK26.5.
Native Weston/Niri/Relay entry points remain linked. Exact isolated development
phone/watch copy passes deep strict signing; graphics/Mode A gates pass.
Duplicate cube HUD, demo main, wpm_main and rust_eh_personality warnings remain.
Physical runtime/distribution gates are open; STARDUST's earlier DDI lock persists.

Correction: initial direct scoped Miri invocations omitted MIRIFLAGS, so their
strict-provenance labels were premature. Explicit -Zmiri-strict-provenance
reruns pass pairwise1 and vsock7 tests. Full32-suite script is being rerun;
only its completed result may establish current full-suite acceptance.

## 4 KiB traffic restored and shared Rust bridge (2026-10-01)

Refreshed4KiB image loads PF_VSOCK during sysinit, publishes its diagnostic
TRANSPORT_STARTED marker, sends REQUEST then16+28 waypipe bytes from guestCID3
to hostCID2:1024, and reaches Multi-User. Explicit transport loading removes
the previously observed reset in this measured run. Marker and byte traffic
remain transport evidence only; no authenticated readiness or imported frame.
hvc0/getty and absent optional OCI share still fail; coldplug takes minutes.

StreamBridge is a shared Rust core over already-connected UnixStream endpoints,
no new dependency or hidden worker. At most64KiB per direction; bounded
nonblocking advance preserves bytes through backpressure, drains before FIN,
keeps reverse traffic after EOF, and closes owned endpoints on cancel/drop/error.
Darwin SHUT_RD did not make writes fail; fatal-error fixture now drops the peer,
with the same required error/closure assertions. Five real socket/range tests
pass, including simultaneous backpressure with differently sized patterned data.
Kani proves actual consume helper for arbitrary usize inputs; Verus separately
models valid-range conservation. These do not prove OS scheduling or auth.
Native waypipe listener connection/launch, private-path lifecycle, session stop,
authentication and frame ownership remain integration work. VZ still needs
direction/readiness repair and migration away from its legacy Swift byte pump.

Fresh304 workspace/all-targets tests (245 VM),26 Kani/15 Verus pass. Clippy
correctness/suspicious, format and iOS13/SDK26.5 relay-ffi release compile pass.
Full32 strict-provenance Miri suites passed before adding this core; the new
pure range suite passes explicit strict Miri. Current configured suite count33;
do not label this as a fresh full33 run. Signed full pairwise app above predates
the unused new bridge core. A new product link is required with host integration.
Evidence: host-tests-stream-bridge.log, miri-full-pairwise.log,
miri-stream-bridge-strict.log, clippy-stream-bridge.log,
ios-stream-bridge-library.log, guest-4k-module-load.log.

## Fresh FMUL/SHM guest probes (2026-10-01)
Both new guest images built successfully; immutable paths and hashes are in
Wawona/.artifacts/relay-build/guest-shm-evidence.json. Fresh release probes
completed900 wall seconds with exit0 and no unsupported instruction/kernel
panic. This is liveness evidence, not target, authentication or frame acceptance.
4KiB completed8,478,670,848 instructions but remained in coldplug;16KiB
completed9,396,600,832 and reached Basic System, then waited on DHCP/networking.
Both /dev/hvc0 device jobs timed out before coldplug completed. Console output
itself works: TX completions1467/1673 respectively, RX256 posted/0 consumed.
4KiB: coldplug starts132.193 guest seconds, hvc0 times out208.713, udev naming
message222.437.16KiB: coldplug starts115.368, naming188.310, hvc0 timeout190.355,
coldplug finishes274.946. These traces do not establish a missing console device
or prove a clock defect. Optional OCI share mounts fail without virtio-fs.
Evidence: guest-4k-shm-fmul.log and guest-16k-shm-fmul.log. Next diagnostic
probes add only udev.log_level=debug and systemd.log_target=console to copied
manifests, run1200 wall seconds, and retain existing guest service timeouts.
No image alteration or acceptance relaxation. Simulator full build remains live.

## Bounded CPU and native shutdown (2026-10-01, validation pending)
StaticCpu Stop previously removed its registry entry and joined the CPU without
an observation deadline before the already-bounded native worker join. A delayed
CPU could block native UI indefinitely and concurrent calls could observe an
absent handle during incomplete shutdown. Stop now keeps the session registered
until both owned threads have joined, with one shared two-second observation
deadline. A pending CPU retains its JoinHandle, socket peer and exclusive disk
file; retry continues cleanup. A CPU panic marks the session exited and retains
native cleanup ownership until retry. Native worker uses the same join helper.
Two real-thread tests cover delayed CPU peer closure/native EOF and panic reaping.
These do not prove OS scheduling, arbitrary foreign callbacks, registry-wide
fairness, app Stop or guest filesystem preservation. Waiting for the registry
mutex is outside the join observation deadline. Existing formal invariants still
apply; fresh Kani/Verus/host test run is pending in host-tests-bounded-shutdown.log.
Current Simulator build snapshot predates this change; verify that artifact's
FMUL/SHM/exit behavior, then rebuild the final app with bounded shutdown.

## Verified shutdown and current Simulator artifact (2026-10-01)
Fresh316 workspace/all-targets tests (257VM),26 Kani/15 Verus, full36 strict
Miri suites and Clippy correctness/suspicious pass, with existing Clippy warnings.
Formal digest7d6e5eb197c0591e29fcd5d0ecdef702a27fe78219043712070a39873de477e7.
The added real registry test proves a pending CPU retains the handle and disk
lock, forbids concurrent disk growth, then allows retry/reopen/growth with the
same persisted test bytes. This is host lifecycle evidence, not app/guest data
acceptance. Pure thread retention/panic tests run under strict Miri; native
UnixStream/foreign-entry checks remain host concurrency tests. iOS13 relay-ffi
release compilation passes. Full new bounded-Stop app linking remains pending.
Evidence:host-tests-bounded-shutdown-final.log, miri-bounded-shutdown.log,
clippy-bounded-shutdown-final.log, ios13-bounded-shutdown-library.log.

Full Simulator phone/watch app build exec77494 finished exit0. Exact output
/nix/store/naglmk381ys88llg9ccvsc2wzvm7qzvl-Wawona/Wawona.app includes both fresh
Image/rootfs/manifest sets and real Weston/Niri/Relay/native host-waypipe symbols.
It predates bounded Stop. Current Simulator phone main/Model/UIContracts floor
14.0, watch companion/frameworks10.0, all SDK26.5. A minimal SDK26.5 clang link
requesting arm64-apple-ios13.0-simulator also stamps14.0. Scoped
--mode-a-simulator checks platform7/native arm64/floor14.0; device --mode-a keeps
platform2/floor13.0. The wrong-platform Simulator is rejected by the device gate.
Simulator SwiftShader is the real namespaced static source provider. Graphics
gate requires its three public entry points plus driver-owned identifiers when
no dylib exists. Device exclusion is unchanged. Both real artifacts pass their
scoped gates; the real device bundle is rejected as a Simulator without its ICD.
Deployment records:simulator-guests-deployment.json; compile probe:
simulator-floor-probe.log. Simulator bundle matrices still need runtime evidence.

Nix-store install returned EACCES. A complete writable ditto copy at
.artifacts/relay-build/simulator-fmul-shm/Wawona.app installs via agent-device and
launches; no uninstall or profile clear. Default profile remains; new QA is
55BBF8D8-AF7A-4C9D-8801-A36237C03895, Relay Simulator QA 4K. Native create/save/
reopen retains768MiB/9GiB. Persisted globals confirm TouchInputType Multi-Touch
and TouchPointerEmulation false. Actual boot trace confirms805306368 memory
bytes and9663676416 backing-disk bytes. Current real Simulator boot is live;
no imported frame/authentication/Stop/grow/restart/guest-data claim yet. Card
subtitle incorrectly hardcodes Relay VZ; launch actually uses StaticCpu.
Simulator data root:
/Users/8amps/Library/Developer/CoreSimulator/Devices/AA38C3A4-C022-41DB-8960-4050F3DE014B/data/Containers/Data/Application/10FA414C-5675-4A98-AC34-38717587B90A
Boot console under Library/Application Support/Wawona/relay-state/machines/<id>/console.log.

Both1200s diagnostic probes finished exit0 with no unsupported instruction/panic.
16KiB reaches Multi-User System and sends real16-byte vsock payload; the diagnostic
never replies as waypipe or authenticates a frame.4KiB has not reached required
target. hvc0 is actually queued by udev only at365.445s/273.589s respectively,
after device/getty timeout.16KiB udev READY=1 is203.367s;4KiB238.353s. These are
measured delayed discovery, not proof of a broken console model. Capture queue/
worker completion and direct-root stage1 omission before changing device/time.


## Measured vector rounding and stage1 discovery (2026-10-01)

Simulator QA4K saved/reopened768MiB/9GiB and actually booted with those values.
Its direct-root guest stopped at937227ms/11341254656instructions on
0x2e218bff(FRINTA V31.2S,V31.2S), not a timeout or app crash.
Evidence: simulator-4k-fatal-vector-round-trace.log. A portable aliased/inactive
sNaN regression reproduced failure before repair. Vector FRINT N/P/M/Z/A/X/I
now reuses the existing unsigned lane and software integral-rounding kernels
for21 S/D forms, snapshots sources, clears Q0 upper bits, accumulates FPSR and
rejects reserved Q0/D.190848 native comparisons and319workspace/260VM tests
pass;26Kani/15Verus and all37strict Miri suites pass. Clippy correctness/
suspicious and explicit iOS13 relay-ffi release compilation pass. Formal digest
6e3c2470a9d4d631d7a4cdd4da10c970bcbe2301b239a83ed6b26a2cd3e2a6cf.
Existing lane proof/model covers selection/bounds, not decoder/FP refinement.

Complete bounded-Stop/label Simulator app built: x90idc4lk2w1fvaz4vraa68md00gwgcy-
Wawona, scoped ModeA/graphics gates pass. It predates vector-rounding repair.
New full vector-rounding app builds via exec94787; no final-artifact claim yet.
Real unchanged initrd probes exec71036/92681 run1200s; their executable/source
version is pinned by real-stage1-launch-provenance.json.16KiB processes hvc0
in stage1 then reaches Multi-User but still times out its device/getty job.
Stage1 alone is insufficient. Pinned NixOS stage-1.nix omits99-systemd.rules;
upstream's serial-console rule tags hvc0 for systemd. New guest recipes bundle
the real initrd/hash, hand off to the actual NixOS stage2 path, reject cpio
case-hack paths and copy exactly the upstream console rule in scripted stage1.
NixOS MCP queried; internal extraUdevRulesCommands is absent from public index
but present in pinned module. Use config.systemd.package for rules, not
pkgs.udev(which is only minimal libs); initial build exposed that output error,
corrected. Both new images building(exec78291); service/guest-frame acceptance
remains pending. No deadline/unit reduction. Auth/OCI/devices/distribution open.


## Current verified artifacts and live jobs (2026-10-01)

Vector-rounding Simulator app built(exit0):
/nix/store/9ksfzz1c130ljnn5ip4ix0484dh80iqi-Wawona/Wawona.app.
ModeA platform7/floor14 and real static graphics gates pass. Not installed yet.
Both new stage1/console-tag guest bundles built(exit0), every kernel/initrd/
rootfs byte count and SHA256 verified, cpio case-hack paths absent, exact upstream
non-remove serial-console tagging rule present. guest-stage1-console-evidence.json.
4KiB:/nix/store/i98b3zprqnlzmbvgqhara9yariv32ypb-wawona-mobile-guest-artifacts,
16KiB:/nix/store/k06nc5y75cvh2jlbnd27lc6i5li55i3m-wawona-mobile-guest-artifacts.
Earlier real-initrd-only probes finished1200s(exit0);16KiB reaches Multi-User
but hvc0/getty fails;4KiB startsPID1 but does not reach requiredtarget.
Fresh exact recipe probes exec13675(4KiB)/9706(16KiB),1200s, now use current
vector-rounding CPU and no udev-debug override; observe actual guest streams
only. Logs guest-4k-stage1-console.log/guest-16k-stage1-console.log.
Full Simulator build including these new guest images remains live exec1286;
logcurrent-build-simulator-stage1-console.log,outlinkcurrent-app-simulator-stage1-console.
Installed Simulator app remains old FMUL/SHM/exit version; actualCPU halted at
measuredFRINTA. Preserve existing QA profile/disk. New initrd command uses new
NixOS toplevel path; an existing disk may retain old closures. For next runtime
acceptance use a fresh QA machine; do not reseed/delete the old disk. Explicit
guest-version pin/migration and preservation across app upgrades remain open.
Storage.open currently keeps an existing disk without base-image-version binding;
this source inspection is not a measured failed migration.
All319tests/26Kani/15Verus/37Miri,Clippy correctness/suspicious and iOS13release
library checks have terminalsuccess. No authentic readiness/frame acceptance.

### Simulator profile and Swift runtime integrity (2026-10-01)

The stage1/tagged-guest Simulator app installed successfully. A fresh QA
profile saved and reopened with 768 MiB RAM and 9 GiB storage. All three
profiles survived relaunch. The previous 9 GiB disk kept its exact SHA256
across installation (simulator-stage1-install-preservation.json). This proves
host disk preservation, not guest application data or restart acceptance.

Save left the Machines list stale until relaunch. Native profile persistence
now broadcasts the existing profiles-changed notification to all models.
Start taps on the fresh VM produced no new disk or visible transition. Native
launch failures now have a visible error alert and a diagnostic log; the full
Simulator rebuild is pending. Do not claim a VM is running from a successful
automation tap.

Xcode's built-in CopySwiftLibs produced a zero-byte libswift_Concurrency.dylib
in both measured Simulator and older physical-device artifacts. The previous
deployment-floor loop silently skipped it. The artifact gate now rejects
empty or non-Mach-O embedded dylibs. The original Simulator app fails this
stronger gate; a separate unsigned copy with the real Apple arm64 runtime
passes. The standalone Apple swift-stdlib-tool restores the real library even
when its destination starts empty. The unsigned iOS Simulator recipe applies
that copy and retains the actual arm64 slice. Signed/archive products are not
modified by this workaround; exact signing/export repair remains open. The
underlying built-in copier failure is not yet explained.

Both exact 1200-second guest probes finished without unsupported-instruction
failure. Both start the hvc0 serial getty with the upstream console tag. Both show
60-second login retries; the 16 KiB trace eventually reaches a genuine wawona
shell prompt, while the 4 KiB trace has no shell prompt. Neither trace proves
the required Multi-User target, authenticated readiness, or an imported guest
frame. An OCI bundle
mount failure is also visible. Preserve these failures as acceptance debt.

### Profile reload loop and final runtime packaging (2026-10-01)

The launch-error/notification app compiled and linked. Its raw Nix bundle still
failed the stronger gate because its Swift concurrency library was empty.
A separate unsigned install copy with the genuine Apple arm64 runtime passed
both Mode A and graphics checks and was installed. Simulator data moved to
073FFAFE-A187-4EDE-8881-CBC94761E369; all three profiles and Multi-Touch settings
were present. No fresh QA boot files were created. Automation later hung while
waiting for this app to idle, then failed to launch its runner. The runner was
reinstalled through agent-device and the app closed; no VM boot was interrupted.

Source and actual stored profiles reveal a reload cycle: serialize always
recreated bundledAppID="" and useBundledApp=false for VM profiles; loadProfiles
removed those keys and saved; the new change notification scheduled another
reload. The native persistence adapter now writes those fields only for native
and Wasm profiles, and compares parsed saved values before publishing an
unchanged save. Runtime proof of Save/reopen/Start awaits the full rebuild;
do not claim the UI loop is resolved from source inspection alone.

Conditional Swift repairs in buildPhase and postFixup did not produce valid
final libraries. An unconditional final Apple-copy plus arm64 extraction and
integrity assertion did. The completed Nix app at
/nix/store/qqwg07m7fidhl6zvkjj97k97kma58gcf-Wawona/Wawona.app has the real
557440-byte arm64 library and passes the stronger Mode A dependency gate.
The reason earlier conditional checks skipped the repair remains unexplained.
Signed/archive products are outside this unsigned Simulator workaround.

## Simulator resume (2026-10-02)

The profile-idempotent Simulator build completed successfully. Its immutable
app at /nix/store/lj0p4l8x7jf3n18spy1ynfqjcgn85gn4-Wawona/Wawona.app passes
Mode A and graphics gates, contains the genuine 557440-byte arm64 concurrency
runtime, and strongly links Weston, Niri, Relay, WASI and native waypipe FD entry.
Both embedded guest variants match all six manifest artifact hashes.
The exact writable copy was installed; three profiles remained. Save/reopen
responds without the migration/notification loop. Saving a rename still left
the visible card stale until reopening the app. The editor mutates its initial
NSObject; card rendering now takes machineId/name/type value snapshots.
This display change is building; runtime Save verification remains open.

On resume, Machines showed disconnected, the saved name was present, and the
fresh QA console was empty. The earlier session's end is not attributed to a
cause. A new Start through agent-device on DC2A8DBA-EE06-44F1-99AC-F6CBD9B71387
produced a real boot trace with 805306368 bytes RAM and a 9 GiB disk. Stage 1
recovered the ext4 journal, mounted /dev/vda, and entered real NixOS stage 2.
The guest remains live while systemd starts services; no required-target,
authenticated readiness, imported frame, graceful Stop or guest-data claim.
Data container: B8652265-121B-4B32-ABE6-01995123E2B4. Do not reinstall or restart
logging during this boot. A new boot intentionally truncates console.log;
a zero file alone never establishes whether a VM is running or stopped.

Fresh host verification after the UI changes passed 319 workspace tests,
26 Kani harnesses and 15 Verus checks (host-tests-simulator-resume.log).
Previous 37 strict Miri suites remain the last Miri evidence; Rust source is
unchanged. Signed runtime packaging and physical/distribution checks remain open.


The value-snapshot full app build completed at
/nix/store/1zhg80qxnqgm7ingm7rm8z2nzw9gy04j-Wawona/Wawona.app. Mode A dependency
floors and graphics gates passed, all five required native entries remain
strongly linked, and the concurrency runtime remains 557440 bytes. It has not
replaced the app executing the current guest. Runtime rename verification
remains open. Simulator systemd traces expose concurrent device-discovery,
time synchronization, wrappers, OCI-mount and firewall startup jobs; do not
increase timeout limits or drop required services to call the target reached.

## Simulator FCVTN and storage slider (2026-10-02)

The actual 4 KiB Simulator guest reached real Cage/wlroots initialization over
native host waypipe, including host output WL-1 and seat mapping. A real host
Wayland window appeared, but its pixels remained blank. Foot then began and
StaticCpu exited at 959932 ms / 12643827712 instructions on 0x0e616bff.
Static Apple assembly/disassembly identifies FCVTN V31.2S,V31.2D, not integer
saturation. No authenticated readiness or imported guest frame is claimed.
The terminal console is preserved in simulator-resumed-4k-fcvtn-exit-console.log.
The app was closed through agent-device after the CPU exit; that is not proof
of a successful guest shutdown. After a coordinate Close attempt, the tab disappeared but the host screen
stayed blank. The callback and return-to-Machines were not verified; the
guest had already exited. Runtime close acceptance remains open.

FCVTN/FCVTN2 and FCVTL/FCVTL2 double/single vector forms now snapshot aliased
sources and reuse the production integer-only narrow_double/widen_single FPCR
and FPSR helpers. Narrow-low clears the upper half; narrow-high retains the low
half; widen-high selects upper source lanes. Unadvertised half formats still
fail without destination/PC/flag mutation. 70272 statically assembled native
comparisons cover all four forms, aliases, controls, edges, NaNs and random
vectors. Workspace tests pass 322 (263 VM); current Kani26/Verus15 pass.
The proofs retain their existing helper scope, not whole-vector/VM proof.
A new portable vector-precision family is included in strict Miri; full run
and final complete app build are still in progress.

The deployed native WWNVirtualMachineEditorSection used a number field even
though the shared editor already had a slider. Both now use/show native storage
slider values and endpoint captions in GiB. New profiles have minimum4 and
maximum64; editing retains the configured capacity as minimum to prevent shrink.
The native new-profile minimum distinguishes an absent profile from an existing
legacy profile with an implicit8 GiB disk. Saved selection remains integer GiB.
No VM domain engine moved into Swift. Simulator visual/save verification awaits
the full app containing this labeled slider and the current CPU fix.

Current vector-precision verification completed: full38 strict Miri suites,
Clippy correctness/suspicious gate, and explicit iOS13 relay-ffi release
cross-compile all pass. These supplement the322tests/26Kani/15Verus and70272
native comparisons. Final Simulator app compile/link and UI checks remain
pending at this observation. Rust source stayed unchanged during these runs.

## Adaptive machine editor and Nix configuration (2026-10-02)

Machine editor presentation is centralized in WawonaBackport.editorSheet().
iPad uses system page sizing on iPadOS 18+ and the large detent on 16/17;
iOS 13-15 keeps its native presentation. Phone keeps medium/large detents.
The complete hx7kqgisqhhk18mg5160sn4lqslzj0jc Simulator app was installed on
758B28B3-2E6A-46C2-91C3-2B1DCCC0A751. Its real screenshot shows a near-window
page instead of the previous centered medium sheet. Actual labeled native
storage slider was also observed: 8 GiB value, 4 GiB and 64 GiB endpoints.
New icon-only Cancel (xmark) and Save (blue checkmark) are implemented in both
editors, retaining accessibility names and draft-only Cancel. Their final
Simulator Save/discard checks await the newest complete build.

User requests per-machine configuration.nix, flake.nix and relay.nix, with
syntax highlighting and editable packages/Wayland desktop. Relay now exports
nixosModules.relay and a nixos-guest template. The prior mobile guest disabled
Nix; the extracted relay.nix enables Nix, flakes and nix-command while retaining
virtio/rootfs/vsock/OCI integration. configuration.nix owns software/session;
wawona.relay.sessionCommand is an argv list, shell-escaped in the existing
waypipe unit. Evaluated default 4 KiB and user-overridden 16 KiB configs both
retain /dev/vda and enable both features. Guest binaries remain inside Relay.

Rust owns Nix templates and UTF-16 highlight spans; native UITextView/NSTextView
only renders tokens and edits the draft. nixFiles persists through Swift and
Rust profile schemas. UniFFI records support HashMap, not BTreeMap; a complete
app build caught the original wrong map type, now corrected. Shared Swift tests
pass40; Relay passes326 tests plus26 Kani/15 Verus (existing helper scope).
New lexer Miri checks are still running. Final app compile/link and editor
Save/reopen/highlight runtime verification are pending.

This is not a completed guest reconfiguration path: saved files still need
verified transfer, pinned flake.lock, authenticated guest rebuild/status,
last-known-good rollback, and restart into the selected system generation.
Current direct boot cmdline pins a toplevel /init; a future guest rebuild alone
cannot prove persistent configuration across restart. Required target/frame,
networking, physical signed device and distribution gates remain open.

The z408vl53zzkqbf6vw3bfnh6qq7cqs8ds full physical app links but its artifact
gate rejects an empty Frameworks/libswift_Concurrency.dylib. Unsigned iOS
postFixup now selects the real Apple runtime for simulator or phone. Signed
archives are never changed there. Repair verification awaits a fresh artifact.


Physical repair verified: full registered product
/nix/store/c4d89bgl6l09r0as7xw636ad4ss9nmyd-Wawona/Wawona.app retains the real
7.4 MiB Swift runtime and passes Mode A platform2/iOS13 dependency floors plus
iOS graphics policy (real ANGLE/MoltenVK, no SwiftShader/private Mode B).
The fresh plain-byte rewrite resolves the observed post-storage empty runtime.
This is an unsigned product artifact, not exact signed physical-device or
App Store distribution validation. Simulator proof remains product xbcin5qw.


## 2026-10-02: disk-owned NixOS generation boot

New mobile rootfs images now seed /nix/var/nix/profiles/system-1-link,
system -> system-1-link, and /init -> /nix/var/nix/profiles/system/init.
Their manifest uses init=/init rather than pinning the host-bundled toplevel.
This closes the new-image boot-selection prerequisite for guest rebuilds.
It does not implement guest file transfer, flake.lock, rebuild/apply, rollback,
or migration of existing disks. Bundled kernel/initrd compatibility with a
future selected guest generation still needs explicit handling.

Both actual 4 KiB and 16 KiB ext4 artifacts were built and inspected with
native Darwin debugfs: all three links resolve to the expected page-specific
system and its real init file. Receipt: stable-generation-image-links.json.
Fresh host verification passes 326 tests, 26 Kani harnesses, 15 Verus checks.
No production Rust changed in this checkpoint; prior strict Miri scope remains.

Both fresh 600-wall-second static_smoke probes exit0 and remain live, executing
7,646,101,504 (4 KiB) and 8,480,944,128 (16 KiB) instructions. Actual stage2
boots the selected disk generation at 75.970877/66.736714 guest seconds.
Both reach Local File Systems and System Time Set, but neither reaches the
required Multi-User target. Coldplug udev remains pending; /run/wawona/oci-bundle
mount fails and the OCI forwarded-client dependency fails on both guests.
No authenticated readiness or imported guest frame is observed. Smoke exit0
is only liveness. Diagnostic keep_bootcon differs from the release commandline.
Logs and extracted evidence: Wawona/.artifacts/relay-build/stable-generation-*
(including boot-summary.json and host-tests.log).

Phone Simulator QA disk baseline is 9 GiB (9,663,676,416 bytes), SHA256
242c25270f52fb996ed058135f7f82683c81db902c2c0ef519ea6759a2256d39.
The installed phone app still predates the latest VM-first/Nix editor UI.
An accessibility snapshot crashed in Apple's XCTAutomationSession initializer
(Wawona-2026-10-02-133647.ips); screenshot/point Cancel remained usable.
Several successful reported swipes produced unchanged screenshots. The draft
was cancelled without saving; the QA VM is disconnected. No disk growth,
restart, or guest-file preservation acceptance is claimed for this checkpoint.
Do not reset legacy disks or infer runtime storage proof from a host hash.


## 2026-10-02: full coldplug prioritization and crate-local Nix templates

Prior debug traces queue hvc0 only at365.444907/273.589 guest seconds after
module-first coldplug. The exact pinned systemd261.2 upstream trigger requests
--type=all --action=add with module,block,tpmrm,net,tty,input priority. Relay now
adds an explicit asDropin ExecStart reset and uses block,tty,net,input,module,tpmrm.
All subsystems remain included; upstream unit dependencies, serial-console
rules and timeouts remain intact. Both actual4K/16K images build and their ext4
images contain the checked drop-in. Evidence: udev-priority-unit.json,
udev-priority-image-units.json and udev-priority-guests-build.log. Two1200s
static_smoke probes are running; readiness remains unproven until final traces.
No OCI share is supplied by static_smoke, so its optional nofail virtiofs mount
failure alone is not evidence that the regular VM target depends on that mount.
OCI lifecycle still requires a real shared bundle and its own acceptance.

A real isolated crate2nix relay-core build failed on all three include_str paths
because templates outside the crate were omitted. Canonical template files now
live inside crates/relay-core/templates/nixos-guest. Rust includes crate-local
files; the exported flake template points to that directory. The old repository
path remains a directory symlink for compatibility. SHA256 equality proves all
three template contents unchanged. The exact isolated crate now builds:
/nix/store/jf09yhbh1q5bisj6394c7bky2lp396p0-rust_relay-core-0.1.0.
Fresh326 host tests,26 Kani harnesses,15 Verus checks pass after the move.
iOS13 compilation and strict editor Miri remain running. Evidence:
relay-core-isolation.log (reproduction), relay-core-isolation-fixed.log,
relay-core-template-content.json and udev-priority-host-tests.log.


Template packaging validation finished: all4 strict-provenance nix_editor Miri
tests pass (360.93s), and relay-ffi aarch64-apple-ios release compilation with
IPHONEOS_DEPLOYMENT_TARGET=13.0 passes. Current formal source digest:
739cab04819bbd51ccfadee30a6c00ad99cecae0006283d36f7fff51e0c809e8.
This is Rust-library compilation, not a new complete signed app or guest-apply
proof. Full device/Simulator app artifacts described earlier predate this move.


## 2026-10-02: completed coldplug-priority runtime probes

Both1200s probes finish exit0 without unsupported instruction or kernel panic.
4KiB executes15,015,571,456 instructions;16KiB16,570,294,272. Both actually
complete coldplug, reach Basic System, start Serial Getty on hvc0, and start
the real Wayland session. Transport-start console hints occur501.591290/413.780593
guest seconds; both send a real16-byte waypipe vsock payload to host port1024.
Neither trace proves Multi-User. Both optional OCI mounts fail without a share;
16KiB additionally reports failed session-1.scope/session-2.scope. These failures
remain acceptance findings. No authentication or imported guest frame is claimed.
Exact extracted receipt: udev-priority-boot-summary.json. Probe executable and
pre/post template-move source provenance: udev-priority-probe-provenance.json.
This narrows the earlier console/getty failure; it is not proof the priority
change alone caused the result because the 600s baseline had less run time.

Detailed service traces exec59164 and exec11042 both finished exit0 after
1200 wall seconds. Logs: udev-services-trace-4k.log and
udev-services-trace-16k.log. systemd 261 prints "Startup finished" when the
job queue is empty, not when multi-user.target becomes active. Neither log
contains "Reached target Multi-User System" (the description appears only in
the initial queue line). Do not treat Startup finished as the required target.

Measured on both page sizes, with service deadlines unchanged:
- Optional OCI virtiofs tag oci-bundle is absent, so the nofail mount fails
  and wawona-container fails on dependency. Expected for static_smoke.
- dhcpcd is Type=forking with waitip and there is no virtio-net. It stays at
  "Starting DHCP Client", then the daemon reports no interfaces and times out.
- shadow login arms LOGIN_TIMEOUT (default 60s) before PAM. Autologin still
  waits in pam_systemd for user@1000. 16 KiB showed that job at 52s of a
  1min 33s deadline when login printed "Login timed out after 60 seconds".
  The session leader exits, and session-N.scope then fails with result
  'resources' ("No PIDs left"). 4 KiB does this too. 16 KiB also repeats it
  for sessions 4 and 5, and user@1000 deactivates.
- 16 KiB only: early "bpf-restrict-fs: Failed to load BPF object: No such
  process". 4 KiB attaches that LSM. Not the shared missing-target cause.
  Kernel rebuild not taken for this.

relay.nix now sets security.loginDefs.settings.LOGIN_TIMEOUT = 0 (prompt
alarm only; user@.service TimeoutStartSec unchanged) and disables dhcpcd
until virtio-net exists. Eval keeps the other login.defs keys and drops the
dhcpcd unit.

Rebuilt images keep the same kernels and replace the rootfs:
4 KiB /nix/store/qrkhvnr0zscw73zqic6mh5qgd6r6mfgx-wawona-mobile-guest-artifacts,
16 KiB /nix/store/x6p6wnhkj78586af0p6ly7hblz6f720b-wawona-mobile-guest-artifacts.
login.defs is /nix/store/yilb5nja78rwgk3572k3lz26gh4n0whc-login.defs
(LOGIN_TIMEOUT 0, DEFAULT_HOME, ENCRYPT_METHOD, UID ranges). dhcpcd.service
is absent. Getty still autologins wawona.

1200s re-probes login-scope-trace-4k.log and login-scope-trace-16k.log both
exit 0. Gone on both page sizes: "Login timed out", session-N.scope result
resources, dhcpcd timeout. The shell prompt stays. logind creates sessions 1
and 2 plus user-manager session 3. Network and Basic System still reached.
OCI mount still fails (no share). 16 KiB bpf-restrict-fs still returns ESRCH.
The Wayland session still prints WAWONA_RELAY_TRANSPORT_STARTED and sends the
16-byte vsock payload. "Started Wawona mobile Wayland session" is still absent.
Those first re-probes used systemd.log_target=console. systemd 261 then
skips the journal copy of status lines (job.c console_only). fbcon takes
/dev/console at about guest 476s, before the target becomes active, so the
status line never reaches hvc0. systemd.show_status=yes does not change that.

Journal-forward probes (forward_to_console=1, no log_target=console) print the
required line on both page sizes:
4 KiB guest 487.507676 "Reached target Multi-User System" after
"Started Wawona mobile Wayland session" at 487.489143. Startup finished
1min 49.709s kernel + 6min 15.875s userspace.
16 KiB guest 406.996742 the same target line, session started at 406.334490.
Startup finished 1min 36.308s kernel + 5min 10.736s userspace. A later
"Startup finished in 1min 1.866s" is the user manager, not pid 1.
Logs: login-scope-journal-4k.log and login-scope-journal-16k.log. Both
remained live for 1200s and exited 0.
OCI mount still fails with no share. 16 KiB bpf-restrict-fs still returns
ESRCH. Both still send the 16-byte vsock payload. No authentication and no
imported guest frame.

Current compile receipt confirms Relay dylib platformIOS,minos13.0,SDK26.5:
template-isolation-ios13-build-version.log. Actual new complete app linking,
existing-disk migration, per-machine growth/restart/guest-data acceptance,
flake apply/rollback, authenticated graphics, OCI/device lifecycle, exact signed
physical-device/distribution checks and guest AOT remain open. Phone Simulator
still has3 saved profiles and QA disk9GiB; no new profile or growth was saved.

## 2026-10-03: host NixOS generation selection

The host menu selects a generation the way systemd-boot does, then Relay
boots that generation with `init=/init`. SwiftUI (Machine Settings and the
machine editor) lists `/nix/var/nix/profiles/system-N-link` from the stopped
disk and stores `nixosGeneration` on the machine profile. On the next start,
Relay rewrites only the fast `system` symlink to `system-N-link`. The command
line is not rewritten. A missing profile, a dirty journal (`needs_recovery`),
or a generation that is not on the disk fails the start and leaves the disk
unchanged. "Current profile" omits the field, so existing disks boot as they
are.

The bundled kernel and initrd still come from the guest manifest. A generation
whose bootspec names a different kernel is not loaded yet. Compose and GTK do
not have a VM generation control yet. They share the same `nixosGeneration`
profile field and `relay_nixos_generations` C ABI. Synthetic ext4 tests cover
list, activate, dirty-journal refusal, and inode checksum rewrite. This is
not a guest rebuild, not a real-disk proof, and not Multi-User acceptance.

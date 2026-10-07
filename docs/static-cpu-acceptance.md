# StaticCpu acceptance gates

Tests describe required behavior before implementation changes. A failing gate
stays failing until independent evidence demonstrates the required behavior.
Passing lower gates never implies passing higher ones. Do not replace an
expected failure with an ignored test, a weaker assertion, or a fabricated
readiness/frame event.

| Gate | Required evidence | Reject |
| --- | --- | --- |
| CPU memory semantics | 4 KiB and 16 KiB discontiguous physical mappings, every SIMD page split against native AArch64, scalar widths, second-page permission fault | Physical-neighbor reads/writes, skipped COW fault |
| Trace integrity | Recomputed state/chain hashes, contiguous checkpoint sequence, increasing step counts, valid register shape | Empty, changed, missing, duplicate, reordered checkpoints |
| Linux PID 1 | Real kernel console reaches `Run ... as init process`, process remains viable | Interpreter merely running, panic loop |
| NixOS Stage 2 | Stage 2 completes with no loader corruption and reaches the real systemd process | Stage 2 banner alone, missing libraries, undefined symbols, segfault |
| systemd target | Guest reports target active and no failed required units | Boot log substring without unit-state evidence |
| Automatic login | Guest session belongs to the intended `wawona` account without host terminal intervention | Login prompt alone |
| Readiness | Authenticated vsock receipt bound to machine/session and verified guest artifact hashes | Console `WAWONA_RELAY_READY=1` string alone |
| Wayland | Guest client via waypipe, compositor surface import, observed frame hash | Socket exists, placeholder surface, host-generated frame |
| Interaction | Keyboard, touch, resize, clipboard, audio, reconnect and shutdown exercised through app UI | API call success without visible/guest-side result |
| Page-size parity | Same complete acceptance sequence on 4 KiB and 16 KiB guest bundles | Unit tests treated as whole-guest boot proof |
| OCI lifecycle | Verified immutable layers, guest crun create/start/stop/kill/restart and corruption recovery | WASI program labeled a Linux container |
| Distribution | Exact signed artifact audited, device run, App Store processing/review evidence | Simulator result treated as distribution acceptance |

## Current measured evidence (2026-10-06)

Headline completeness: **0% completely complete** for the iOS Mode A Linux VM
product. Twelve-gate progress is **~59%** (judgment). iOS `vm` remains
**planned**. Scorecard and remaining order:
`static-cpu-completion-plan.md` (2026-10-06).

On proof machine `E4A1C0DE` / guest `guest-4k-auth10` (auth18 log):

- `WAWONA_RELAY_READY=1` at guest 497.72s
- `Relay imported SHM frame: 800x600 sha256=5b6add912fb4447127f249f8f924379f8928e81f6fc5fc6ed4cf3d694a7379c3`
- OPEN_FILE remote=1 bytes=15000000 before the frame
- Prior auth16/17 also had `WAWONA_RELAY_AUTH_OK=1` and `dns ok`
- `nix run` / `wawona-fastfetch: end` still open (TLS EOF on prior boots)

Formal: `verify-formal.sh` `FORMAL_OK`, 37 Kani harnesses, 0 failures, stamp
digest `0e20b45b2e06b209a37888048e2a5b9aad479a866f60efa473d0ff05afa23628`.
Helper cargo tests `shm_import`, `aot`, `net_host` pass after that stamp.
Named helpers only. Disk grow/restart with guest marker already passed on this
machine. READY without AUTH still rejected. AUTH without an imported frame
still fails the Wayland gate. Simulator is not distribution.

## Historical measured evidence (2026-09-30)

Latest checkpoint: both page sizes complete Stage 2 and reach real systemd
service startup. Required target, authenticated readiness, Wayland and app/device
acceptance are still open. Current local gates pass: 281 workspace tests (225 VM),
22 Kani harnesses, 11 Verus checks, Clippy correctness/suspicious checks,
an iOS 13-targeted release-library build. The shared WawonaUI package also
compiles and links for iOS 13 with SDK 26.5; full app/device validation is open.
All 26 strict-provenance Miri suites pass. Both granules remained
live for 480 seconds without an unsupported-instruction stop; this bounded
observation is not successful boot acceptance. The 16 KiB console exposed
`iptables: Failed to initialize nft: Protocol not supported`; the kernel
recipe enables the required Netfilter dependencies and the refreshed 16 KiB
guest builds. Both refreshed guests remained live for 600 seconds. Service timeouts and
missing required readiness still prevent boot acceptance. Pure reduction, table-lookup and saturating lane kernels have exact Kani
contracts; these do not establish whole-CPU correctness. The feature-gated kernel
probe is excluded from default production builds.

Cross-page scalar/SIMD memory accesses must translate every virtual fragment:
adjacent virtual pages may map discontiguous physical frames. Validate both
fragments before split stores so second-page permissions fault without partial
writes. Native SIMD tests cover every split for 4/16 KiB mappings.

16 KiB Linux needs TGran16=1 (TGran4=0, TGran64=15) and TxSZ-derived starting
levels per TTBR. Measured TCR_EL1=0x045000757551b510 has T0SZ=16 (four-level
48-bit low half), T1SZ=17 (three-level 47-bit high half). Mask canonical high
bits before initial lookup. Hardcoded four-level walks cause PC=0x200 loops.

Differential traces are untrusted evidence: recompute state/chain hashes and
reject empty/malformed/reordered checkpoints. `instructions` counts interpreter
steps, including synchronous exception entries; it is not retirement. No
full-system reference adapter ships yet. Self-replay proves determinism only.
`static_smoke` must reject loader errors, panics and interpreter failures even
while a thread stays alive. Acceptance: `Relay/docs/static-cpu-acceptance.md`.

`cpu/native_reference.rs` runs fixed, statically assembled AArch64 instructions;
no guest bytes execute and no executable memory is generated. FP tests restore
host FPCR/FPSR/NZCV and compare guest results plus exceptions. Coverage includes:
UMOV/SMOV, DUP, INS, MOVI/MVNI/ORR/BIC/FMOV immediates, precision and integer FP
conversions, scalar FP arithmetic/comparisons, vector ADD/SUB/comparisons,
pairwise reductions, bitwise/unary byte operations, widening shifts, EXT, XTN.
Modified immediates cover all 16,128 valid encodings with three destination
patterns; scalar FP immediates cover 512 encodings; INS covers 370 lane forms.
Use non-inlined const-generic helpers for large corpora: macro-expanded debug
stack frames overflowed in the initial exhaustive immediate test.

Software FP uses pinned pure-Rust rustc_apfloat 0.2.3 with ARM-specific NaN
priority, FZ/DN, rounding and status. APFloat omits OVERFLOW on directed
saturation: native tests caught it; exact S/D→Quad widening and wider-range
evaluation recover ARM's flag. Native tests also caught old host-float casts
and compares losing FPSR flags. Never restore those shortcuts. Arithmetic
samples include normal/subnormal rounding boundaries. Formal obligations cover
named invariants, not the entire FP library or CPU.

Measured boot: 4 KiB direct-root reaches NixOS Stage 2 activation; 16 KiB
PL011/nokaslr diagnostic reaches real systemd-udevd startup. Neither establishes
systemd target, authenticated readiness or Wayland. Latest full suite at this
checkpoint: 166 VM tests, 3 smoke tests, 11 Kani harnesses, 8 Verus obligations.
iOS aarch64 cargo check passes; portable arithmetic and reserved-encoding
regressions pass Miri. Native assembly cases are excluded under Miri/non-AArch64.

## Follow-up checkpoint

Latest CPU suite: 187 VM tests and 3 smoke tests; 13 Kani harnesses and
9 Verus-reported checks. Fetch privilege/XN/table permissions now have explicit
production checks and 4/16 KiB regressions. FRINT scalar forms and scalar D
integer comparisons extend the native corpus. Expanded Miri suite passed;
ARM64 CI is configured but has not yet run remotely. The iOS arm64 archive was
rebuilt after fetch/FRINT work; later source changes require another build.

The complete gap analysis, proof boundaries and execution tracks are in
`static-cpu-completion-plan.md`. No higher boot gate is claimed at this checkpoint.

## Shared-family and page-fault follow-up

Both rebuilt direct-root artifacts now start real systemd as PID 1 after
activation and `/etc` setup. The 16 KiB artifact no longer depends on the old
initrd udev wait. This does not establish a reached systemd target, authenticated
readiness, or a working Wayland session.

The measured SADDW miss led to all 48 signed/unsigned long/wide add/subtract
forms, checked against fixed native instructions. The subsequent LD1 miss led
to a shared LD1/ST1 implementation covering 192 native arrangements/addressing
forms. The four-register transfer reuses the checked RAM path, now bounded at
64 bytes; every page split, register wrap, and fault-before-writeback is tested.
Kani and Verus fragment bounds were widened to match. The next measured miss,
SHL `0x4f2c5621`, has a native corpus covering 11 non-saturating immediate-shift
families at minimum/middle/maximum shifts, including scalar D forms.

Software-managed Access Flag faults now precede permission checks and retain
the actual level. HAFDBS remains unadvertised. Page/block tests cover read,
write and execute, descriptor preservation, and fault-state propagation.
Reference: [Arm memory attributes, section 11](https://documentation-service.arm.com/static/63a43e333f28e5456434e18b?token=).
This is not a complete descriptor-legality or MMU proof.

The local proof runner compares source digests before and after verification
and refuses a new stamp on change. A stubbed runner fixture verified this gate
behavior; the real Kani/Verus suites remain separate evidence. The stamp still
is not an immutable-build or signed release attestation.

### Privileged hardware reference

58 fixed hardware observations (29 cases, two page sizes) now cover selected
MMU permissions, AF priority, SVC/IRQ entry, ERET stack banks and masked virtual
timer status. The production interpreter matches them; 202 VM tests pass.
The reference executable uses Hypervisor.framework only as a standalone macOS
development tool, with no product linkage or guest-image input. Full-system
trace equivalence, systemd target readiness and release completeness remain open.

Validation of the exception-return/timer fixes: the fresh hardware fixture
comparison passed; all 14 Miri suites passed with strict provenance; the release
relay-ffi static library built for aarch64-apple-ios with deployment target 11.0.
The iOS result is compilation evidence, not device-runtime or distribution proof.

### OCI extraction regression (2026-09-30)

An adversarial lower-layer symlink reproduced a write outside the rootfs in a
private temporary fixture before the repair. Extraction now checks real-directory
ancestors, uses tar's confined `unpack_in` path (including hardlink validation),
and checks opaque-whiteout directories. Whiteouts run in a first streaming pass
so they affect only lower-layer data; the second pass adds current-layer entries.
Empty/dot whiteout basenames reject. Tests cover outside writes/deletions,
hardlinks, dangling symlink replacement and late whiteouts preserving new files.
No decompressed-layer buffer or new dependency was added.

This is not race-free capability confinement: concurrent external ancestor
replacement, validated-blob reopening and atomic publication remain open.
Symlink ancestors are conservatively rejected, including internal aliases, until
root-relative confined resolution exists. These filesystem regressions are
native tests; existing OCI Miri coverage concerns pure process conversion.

## Current app and fused-family checkpoint (2026-10-01)

Full phone/watch app links at iOS 13 / SDK 26.5. Its signed copy passes
strict signature checks; physical installation waits for STARDUST unlock.
Fresh workspace/all-targets tests: 287, including imported WPM tests.
22 Kani/11 Verus checks pass; new fused FP has native/dynamic evidence,
not a new mathematical whole-FP proof. Two targeted Miri suites pass.
Scalar S/D FMADD/FMSUB/FNMADD/FNMSUB cover eight fixed native forms and
812,032 comparisons. The 16 KiB guest passes its old FMADD stop and reports
Multi-User System with a wawona user session. Required Wayland service
fails with missing vsock device / missing remote wl_compositor. Both earlier
600-second runs expose hvc0 device timeout. Legacy READY console hints do
not satisfy authenticated readiness and can appear after child failures.
The complete required-unit target, guest-frame and interaction gates remain
open for both granules. Build map: Wawona/docs/ios13-product-build-map.md.

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

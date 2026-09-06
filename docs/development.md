# Development and evidence gates

## Workspace

Isolated worktrees of Wawona, wwn-wasm, wwn-vms, and wwn-containers use branch
`runtime/static-engine` under `runtime-work/`. Original development checkouts
and their uncommitted work remain untouched. This new local repository is
`wwn-runtime`. The GitHub repository `Wawona/wwn-runtime` has been created
privately; public visibility is awaiting the owner's preference.

## Delivery sequence

1. Validated static execution, reference semantics, real WASM input, reproducible
   microbenchmarks. Initial implementation exists; optimization work continues.
2. Extend WASM coverage: control flow, calls, memory, integer widths, floating
   point, tables, references, SIMD, and features required by the existing corpus.
   Run the upstream spec corpus and randomized differential tests against Pulley.
3. Integrate through a backend boundary in wwn-wasm. Preserve the existing P1/P2
   host and Component Model semantics. Fallback selection must occur before guest
   effects; never rerun a guest after a trap or partially completed execution.
4. Generate a measured static superinstruction vocabulary from development
   profiles. Tune dispatch, register residency, operand layout, and code layout.
   Grow the dictionary only when held-out workloads improve, accounting for
   instruction-cache, binary size, peak memory, startup, and energy costs.
5. Add a QEMU TCG backend in wwn-vms, preserving translation-block invalidation,
   MMU behavior, exceptions, interrupts, atomics, and self-modifying guest code.
   Never claim a backend exists merely by adding a launcher acceleration label.
6. Route iOS OCI execution in wwn-containers through the verified VM backend.
   OCI Linux images retain real Linux kernel/container semantics. A future Linux
   userspace personality is a separate compatibility project, not a substitute
   for the requested VM-backed container path.
7. Integrate the actual archives through Nix into Wawona Machines. Build and link
   locally with real Weston/Niri before any CI push. Verify SDK/ABI slices.
8. Run demos in Wawona on Simulator, then physical iPhone/iPad. Verify WASI output,
   Wayland frames/input, VM boot, OCI workloads, cancellation, and repeated launch.
9. Compare with UTM SE and UTM JIT on the same physical device, guest architecture,
   guest image, memory/vCPU configuration, compiler flags, inputs, and outputs.
   Benchmark CPU execution separately from application-level WASI/VM differences.
10. Audit release artifacts, entitlements, execution-memory behavior, public API
    usage, sandbox boundaries, and software distribution rules. Submit the real
    app for review; record Apple's result independently of technical audit passes.

## Performance acceptance

UTM with JIT on jailbroken iOS is the required comparison target. Record exact
UTM/QEMU revisions, iOS/device model, guest configuration, thermal state, power
mode, workload hashes, repetitions, raw times, and correctness checks. Separate
cold startup from warm execution. Publish regressions and uncertainty, plus
per-workload ratios and suite geometric mean. Never use Simulator timings to
claim physical-device leadership. Never infer full-system performance from
recognized native kernels or a dispatch-only microbenchmark.

Claims of first implementation require a dated prior-art review. Claims of
fastest implementation require a stated comparison set and reproducible evidence.

## Verification log, 2026-09-05

- Host engine tests: 7 passed, including 1,323 arithmetic/alias input combinations
  each exercised at five fuel budgets with fusion on and off.
- WASM frontend tests: 3 passed, including real binary execution, local snapshot
  semantics, truncated modules, and unsupported operation rejection.
- Semantic optimization tests: 4 passed, including 4,096 seeded input sets at
  five fuel boundaries, zero-start loops, overflow, aliasing, and observability.
- Upstream Pulley differential test passed on 10,000 seeded i64 input triples
  for the real WASM `(a+b)*c` export. This is not the full WASM spec corpus.
- Mode A serialization test passed: Pulley target flag present; translated
  guest text marked non-executable. All 16 runtime tests and Clippy passed.
- OCI layer work in the sibling container branch reproduced three failures
  before fixing lower-layer whiteout ordering and symlink-parent confinement.
  All 16 existing OCI tests and five new layer tests passed; a 4 MiB layer
  application benchmark also passed output verification on macOS.
- Pinned QEMU 11.0.1 TCI reference built locally after supplying the missing
  libtasn1 dependency required by its TLS qtest compilation. HVF is disabled.
  Three x86 guest runs each verified one million recurrence steps and exited
  through the expected debug-exit status. Cold boot + computation times were
  364,968,666 ns, 278,815,708 ns, and 271,010,125 ns. This is a reference guest
  arithmetic test, not Linux/OCI execution or an iOS/UTM comparison.
- Exploratory macOS arm64 recurrence benchmark (one million iterations,
  11 samples, simulator stopped, other compilation still active): median
  unfused 37,801,458 ns, fused 29,431,917 ns, native iterative 1,538,875 ns.
  Fusion improved this workload about 1.28x; fused remained about 19x slower
  than the native loop. Specialized affine acceleration took median 916 ns,
  including timer overhead, by replacing iteration with exponentiation.
  This narrow algorithmic shortcut cannot establish general VM/WASM speed.
  Raw local samples: artifacts/dispatch-macos-semantic.jsonl (ignored by Git).
- Shared Pulley configuration and explicit static-export path integrated into
  the local wwn-wasm Cargo build; `cargo check --offline --lib` passed.
- Rust P1/P2 demo manifests repaired for standalone builds. Both guest targets
  compiled successfully. Both ran through the shared Pulley engine on macOS;
  subprocess tests verified success and the expected guest stdout.
- Three wwn-wasm integration tests passed: static shell export, P1 startup,
  and preservation of P2 instantiation failures without a P1 retry. The
  cross-compiled-demo test also passed when explicitly enabled (normally ignored).
- Native demo: result 36, one fused pair, three IR operations.
- Initial dependency-free engine compiled as a release static archive for
  aarch64-apple-ios. This is compilation evidence, not device execution.
- Wawona agent-device 0.18.3-wawona.3 was already installed. Registered its MCP
  server in this app's Codex configuration and verified the MCP handshake/tools.
- Wawona iOS Simulator booted and the existing installed Wawona app launched.
  Weston Terminal rendered; Multi-Touch enabled. Typed input was visible, but
  command submission was not verified. That app does not contain this engine.
- The new `wasm_demo` executable ran separately inside the iOS Simulator:
  value 36, one fused pair, host_os ios, host_arch aarch64. This proves simulator
  execution, not app integration or physical-device execution.

- Added structured control lowering and static integer handlers. Eleven new tests
  cover loops, if/else, branch results, function exits, dead code, fuel limits,
  shifts, comparisons, remainders, eager select, bit counts, and sign extensions, with Pulley
  differential checks. Added i32 handlers, wrap/extension operations, and mutable
  i32 locals across loop backedges; 25 binary operators run against Pulley over
  an edge-case input matrix, with both frontends and fusion settings. All 27 runtime tests and Clippy pass locally.
- Local wwn-wasm Nix macOS package now builds with shared runtime source. Full
  WASI CLI compiles for iOS Simulator. P1 and P2 Rust demo guests both executed
  successfully there with their expected stdout and exit status zero. These
  are standalone Simulator processes, not the integrated Wawona app.
- First upstream CI run passes Linux runtime, both Apple target compilations,
  and real QEMU TCI guest execution. OCI validation packaging failed because
  upstream binaries live in validation/*/*.t; corrected that glob. The revised
  VM derivation evaluates locally; Linux guest conformance remains unverified.

Current priority is runtime implementation and Wawona app integration. UTM
comparisons are deferred. Cross-repository source pins and publication access
still need finalization. Do not build release claims from this tree.

No UTM JIT comparison, full WASI integration, QEMU backend, OCI integration,
physical-device execution of this engine, or App Store review has passed yet.

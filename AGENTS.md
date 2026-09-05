# Runtime rules

Wawona-owned implementation is Rust. Preserve the parent workspace rules.
Runtime translation produces data only. Never allocate executable memory,
copy or patch native code, load downloaded native libraries, or enable JIT.
Treat guest instructions and persisted translation artifacts as untrusted.
Never deserialize function pointers. Resolve validated opcodes to local handlers.
Preserve traps, memory effects, branch targets, and execution limits when fusing.
Do not advertise a full WASM, CPU, WASI, or QEMU implementation until its
conformance and end-to-end tests pass. Report benchmark regressions alongside wins.
The target is faster than UTM with JIT on jailbroken iOS using equivalent workloads.
App Store approval and comparative performance are evidence gates, not assumptions.


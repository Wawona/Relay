# Continuous verification

Every push and pull request runs the runtime workflow. Actions use immutable
revisions, read-only repository permissions, timeouts, and cancellation of stale
runs. Hosted Linux and macOS runners test correctness, run Pulley differential
tests, inspect translated bytecode flags, and upload benchmark samples with the
source revision, compiler version, and host identity. iOS archive jobs are
compilation checks; their success does not mean a device ran the archive.

The QEMU lane builds a pinned interpreter-only QEMU reference. Its real x86 guest
computes and checks one million recurrence steps before reporting success. The
timings include BIOS and cold boot. This is neither a Linux boot test nor the
future Wawona QEMU backend. The separate OCI VM lane boots Linux under QEMU TCI
and runs pinned upstream opencontainers/runtime-tools validation executables
against crun. Both process status and TAP results must pass. It preserves TAP
logs and suite duration. A conformance failure fails the job; there is no allowed
failure or fabricated pass. Upstream test skips retain their upstream meaning
and must be examined before making a release conformance claim.

wwn-containers has additional host image-spec regression tests and an image-layer
application benchmark. Host image tests and guest runtime tests cover different
parts of OCI. Neither alone proves complete OCI image/runtime/distribution
conformance. The hosted reference VM does not prove that Wawona's iOS app shipped
or executed the same components.

Hosted benchmark results are exploratory. They do not gate performance ratios
until repeatability on controlled hardware is established. Never compare raw
times across different CPUs or call these results a UTM JIT comparison.

The Mode A artifact test checks the pinned Wasmtime serialization's Pulley flag
and non-executable text marker. It is a technical regression test, not proof
that every shipping app artifact has passed an execution-memory/public-API audit.
Apple's distribution and App Review rules require a separate release review:
https://developer.apple.com/app-store/review/guidelines/

Physical iOS, integrated Wawona Simulator, UTM JIT on jailbroken iOS, App Store
review, and first/fastest claims remain separate evidence gates. No hosted green
badge is a substitute for those results. Linux OCI guest execution is not yet
verified locally on this Mac; no working Linux Nix builder is configured.

## Local reproduction

```sh
cargo test --locked --all-features --all-targets
cargo clippy --locked --all-features --all-targets -- -D warnings
nix build --impure --file ci/qemu.nix --out-link result-qemu
cargo run --locked --release --no-default-features --example qemu_smoke -- ./result-qemu/bin/qemu-system-x86_64
# Linux x86_64 runner or builder required for the complete guest suite:
nix build -L --impure --file ci/oci-vm.nix --out-link result-oci
```

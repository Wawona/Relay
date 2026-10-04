# iOS 13 minimum and bundled-guest AOT assessment

Status: 2026-09-30. The user changed the minimum to iOS 13 and is considering
pure AOT. This document is a proposed design and measurement contract, not an
implemented AOT compiler or a performance claim.

## Current execution

Relay's Rust runtime is compiled ahead of time. Linux ARM64 guest instructions
are interpreted by StaticCpu. Store-mode Wasm uses Pulley. No guest-native AOT
backend exists. Building the interpreter with optimizations does not make guest
execution AOT. The bundled NixOS image still has open boot and graphics gates.

## What pure guest AOT would require

An offline build step would translate the exact bundled kernel and executable
closure into statically linked, signed host functions. Guest CPU state, memory
translation, exceptions and device access must keep their existing semantics;
Linux ARM64 machine code cannot simply execute as iOS user code.

Dispatch must bind translated blocks to verified guest image identities and
original code bytes. Kernel alternatives, jump labels, dynamic loaders,
relocations, indirect targets, code modifications and generated guest code need
an explicit correctness contract. Code changes cannot silently reuse an old
translation. A closed-world mode must reject unsupported executable code, or
use an interpreter and be described honestly as hybrid.

Arbitrary OCI programs downloaded after the app is signed cannot already have
all their host-native translations in that signed app. Keeping that advertised
workload scope therefore requires interpretation or another explicitly supported
distribution mechanism. A pure closed-world AOT product and a general downloaded
Linux-program product have different contracts; do not silently remove either.

LLVM's ORC/JITLink runtime facilities are not an AOT solution for this store
path. Offline compilation can be evaluated without adding a runtime compiler,
executable writable memory, or a second VM engine.

## Adoption gates

1. Finish a repeatable correct-boot and graphics baseline on both page sizes.
2. Profile the bundled workload to select a small translation experiment.
3. Compare the same blocks against native reference execution and StaticCpu,
   including flags, faults, memory effects and invalidation.
4. Keep translation tooling outside the shipped dependency graph. Measure
   linked text/data and total signed app growth as well as speed.
5. Prove the dispatch/image-binding and invalidation boundaries; preserve the
   existing memory/device invariants. No proof count substitutes for refinement.
6. Adopt only after reproducible measurements demonstrate worthwhile gains.

## Performance contract

Use the same device, OS, guest images, RAM/storage, workload, power state and
thermal starting condition. Record cold boot to authenticated readiness, first
real Wayland frame, application launch, interactive latency, steady CPU work,
peak RAM, app/guest size and energy. Publish repetitions, medians, tails and
failures. Separate interpreter, offline-AOT, JIT and hardware-virtualized classes.
Reference-only emulators remain outside the product. No “world's fastest” claim
without a defined comparison set, benchmark artifacts and reproducible results.

## Compatibility evidence

The Relay library builds with IPHONEOS_DEPLOYMENT_TARGET=13.0 using SDK 26.5.
The shared UI's iOS 13 compile exposed newer API use and remains under repair.
A lower deployment setting does not prove API availability or device operation.
Future iOS 27+ releases require their own SDK and device validation. App Store
processing/review is separate from build or interpreter/AOT architecture.

References: [Apple App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)
and [LLVM ORC design](https://llvm.org/docs/ORCv2.html). These explain distribution
rules and runtime compilation facilities; neither certifies Wawona acceptance
or performance.

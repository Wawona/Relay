# Mode A App Store-safe benches

Authority for Relay Mode A speed charts and the `relay-mode-a-bench` app.

## Decisions locked

| # | Choice | Meaning |
|---|--------|---------|
| **1A** | CI uses open-source engine CLIs + in-process class refs | Not `iSH.app` / `UTM SE.app` on GHA. Device lab later. |
| **2A** | Mode A gate = App Store interpreters only | asbestos / unicorn / TCTI class. Badge from `gate.json`. |
| **2B** | Second chart includes JIT UTM | Labeled **cross-class**. Never flips Mode A gate. |

## What is measured

| Suite | Under test | Comparators |
|-------|------------|-------------|
| `page-*` | `PageTranslate` + `GuestMemory` | none |
| `relay-static-cpu` | Mode A StaticCpu-shaped walk | n/a |
| `ref-*-class` | in-process asbestos / unicorn / TCTI-class loops | always on |
| `competitor-*` | PATH CLIs when present (1A) | skipped if missing |
| `cross-class-jit-utm` | JIT UTM / qemu HVF CLI (2B) | skipped if missing; not Mode A gate |
| `microvm-nixos` / `container-in-vm` | skipped until StaticCpu boots NixOS | n/a |

## Claims

- Mode A gate / “world’s fastest” marketing: only vs **App Store-class**
  interpreters, and only after **`streak.json` consecutivePass ≥ 5** CI runs
  (`marketing.json` → `fastest-eligible`). Unmeasured copy is forbidden.
- Cross-class chart may show JIT UTM (or the Nix-pinned `relay-bench-jit-utm`
  proxy) next to Mode A. README must say it is **not** the Mode A gate.
- Never link QEMU/UTM into the Mode A product IPA (bench-only deps).

## Nix-pinned competitor CLIs (1A)

Flake package wraps these onto PATH:

| Binary | Class |
|--------|-------|
| `relay-bench-asbestos` | iSH asbestos-class |
| `relay-bench-unicorn` | Unicorn-class |
| `relay-bench-tcti` | UTM-SE TCTI-class |
| `relay-bench-jit-utm` | Cross-class JIT proxy (2B only) |

External real engines still win if found earlier names are absent and an
alternate is first on PATH. Device App Store apps remain a later lab lane.

## Live charts (no README churn)

| Asset | Role |
|-------|------|
| `mode-a-interpreters.svg` | 2A gate chart |
| `mode-a-page-geom.svg` | page geometry |
| `cross-class-incl-jit-utm.svg` | 2B labeled cross-class |
| `gate.json` | Mode A badge endpoint |
| `streak.json` | consecutive pass counter |
| `marketing.json` | eligibility badge (never unmeasured “world’s fastest”) |

`https://github.com/Wawona/Relay/releases/download/bench-latest/…`

## Local / CI

```bash
nix run path:.#relay-mode-a-bench -- --out /tmp/relay-bench
# cargo (dev only): cargo run -p relay-bench --release -- --out /tmp/relay-bench
```

Flake package uses **crate2nix** (`recipes/relay-mode-a-bench.nix`), same
as `wawona-relay`. Not `buildRustPackage`.

CI uses `--strict-competitors` so missing Nix-pinned adapters fail the job.

## Deferred (not this package)

`microvm-nixos` / `container-in-vm` rows stay SKIP until Mode A StaticCpu
boots NixOS (separate product milestone).

## Page geometry

One Relay VM backend maps 4 KiB or 16 KiB guests onto the host page size
(`PageTranslate`). Guest kernels remain real 4k / 16k Images. Product prose:
Wawona `docs/relay-page-geometry.md`.

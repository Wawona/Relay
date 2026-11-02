# Mode A NixOS guest: KDE Plasma → Wawona Present

Goal: iOS (and every Mode A host) starts a Relay NixOS VM and, when Present
arms, shows **KDE Plasma 6 Wayland** as one Wayland toplevel into Wawona
(iland Metal). Not UTM. Not a downloaded rootfs. App Store 2.5.2: guest
ships inside the IPA.

## Truth today

| Layer | State |
|-------|--------|
| Engine | Relay `StaticCpu` (Mode A) / VZ (macOS) / KVM (Linux) |
| Guest GUI pipe | `waypipe --no-gpu --vsock` → host SHM importer → `IlandPresentEngine` |
| Tip guest session (until full Plasma image lands) | **KWin QPainter** + foot (`kwin_wayland --exit-with-session=foot`). Full `startplasma-wayland` blocked by builder TMPDIR (~2× closure) |
| Closure budgets (aarch64) | plasma6.enable ≈ 7 GiB; plasma-workspace curated ≈ 4.1 GiB; **kwin ≈ 2.0 GiB** (fits Determinated builder) |
| `machine-kde` card | Same guest tree; notes say Plasma; tip embeds KWin session until Plasma fits |
| Terminal in-place (`\r` / ESC M / CSI K) | Landed `Terminal` `0158344` (`wrap_cont`). Host FitTerminal floors cols at 80. Tip backend rebuilt |

## Architecture (end state)

```text
Wawona iOS Mode A
  Machines Start (machine-kde / machine-relay-nixos)
    → Mount session Metal (opacity 0)
    → open Wawona Terminal (guest serial)
    → relay_start(StaticCpu) + bundled wawona-nixos-guest-4k
         Image + initrd + rootfs.img + manifest.json
    → guest: multi-user → wawona-session.service
         waypipe --no-gpu --vsock -s 1024 server --
           dbus-run-session kwin_wayland --xwayland --exit-with-session=foot
           (later: startplasma-wayland when image budget allows)
    → AUTH / READY on serial
    → first SHM frame → ShowSessionSurface
    → Present = KWin nested desktop (Plasma shell when curated image fits)
```

Software path only on Mode A StaticCpu:

- `KWIN_COMPOSE=QPainter`
- `QT_QUICK_BACKEND=software`
- `LIBGL_ALWAYS_SOFTWARE=1`
- `QT_QPA_PLATFORM=wayland`

No guest Vulkan/EGL ICD. Cage previously failed swapchain for the same reason.

## Guest packaging

Source of truth: `Relay/import/vms/dependencies/vms/mobile/configuration.nix`

- **Never** `services.desktopManager.plasma6.enable` for Mode A (~7 GiB)
- Tip session packages: `kwin`, `qtwayland`, `foot`, `weston` (lab) +
  English-only `i18n.supportedLocales`. Target tip closure ≈ 2 GiB
- `services.udisks2.enable = mkForce false` (slim StaticCpu)
- Tip `sessionCommand`: `dbus-run-session -- kwin_wayland --xwayland --exit-with-session=foot`
- Follow-up when builder TMPDIR / ODR allows: add `plasma-workspace` and
  switch session to `startplasma-wayland`
- Keep patched `weston-smoke` in the closure for lab SHM bring-up only
- Flake artifact: `.#packages.aarch64-linux.wawona-nixos-guest-4k`
- Manifest `memory_bytes` ≥ 2 GiB; Machines KDE card ≥ 4096 MiB RAM

Embed into IPA / tip:

```bash
# After nix build:
ditto /tmp/wawona-nixos-guest-4k-plasma/ /path/to/Wawona.app/wawona-nixos-guest-4k
# Store / xcodegen uses mobileGuestIosEmbedScript (WAWONA_MOBILE_GUEST_*_DIR)
```

Never download the guest at runtime (Guideline 2.5.2). Lab may stage the
same tree under Application Support/`Wawona/guests/`.

## Host / UI (full implementation)

| Piece | Behavior |
|-------|----------|
| `machine-relay-nixos` | Generic NixOS Relay VM; same Plasma guest |
| `machine-kde` | Same guest; higher RAM/disk defaults; Plasma-focused notes |
| AUTO_START / Machines Start | Mount Metal → Terminal serial → connect → Show on first SHM |
| Guest resolve | `resourcePath` / `bundlePath` / Application Support |
| Rust fallback | Load `guest_directory/manifest.json` if JSON guest object missing |

### Host call graph (iOS Mode A)

```text
WWNMachinesViewModel.start
  → preflightGuestManifest (public hasGuestManifest / resolveGuestDirectory)
  → RelayRunner.start(spec)
       RelaySpec.resolveGuestDirectory → …/wawona-nixos-guest-4k
       relay_start(StaticCpu) + waypipe SHM importer
  → SceneDelegate / Present host: MountSessionSurface (opacity 0)
  → WWNIosGuestConsole / GuestConsoleView: FitTerminal cols = max(pixelCols, 80)
       feed serial into Terminal wrap_cont VT
  → IlandPresentEngine first non-blank SHM frame
       → ShowSessionSurface (opacity 1, Present visible)
```

### Files that own the contract

| Layer | Path |
|-------|------|
| Guest session + Plasma | `Relay/import/vms/dependencies/vms/mobile/configuration.nix` |
| Slim eth0 / no dhcpcd | `…/mobile/relay.nix` (+ `crates/relay-core/templates/nixos-guest/relay.nix`) |
| Guest memory floor | flake `memoryBytes` ≥ 2GiB; `MachineProfile` KDE ≥ 4096MiB |
| Present Mount then Show | `Sources/WawonaApple/Runners/RelayRunner.swift`, `IlandPresentEngine.swift` |
| Guest path resolve | `Sources/WawonaApple/Runners/RelaySpec.swift` |
| Serial FitTerminal ≥80 | `GuestConsoleView.swift`, `WWNIosGuestConsole.swift` |
| VT in-place rewrite | sibling `Terminal` `src/screen.rs` (`wrap_cont`), flake input `terminal` |
| Design / proof checklist | this doc |

## Phased proof (do not skip)

1. **Guest eval / build** — `nix build .#wawona-nixos-guest-4k` with Plasma config
2. **Embed tip** — ditto into `Wawona-smoke-tip.app/wawona-nixos-guest-4k`
3. **Serial** — Terminal shows systemd → `wawona-session` → Plasma start (no wrap spam)
4. **AUTH / SHM** — importer `rc=0`; frames with non-zero pixels
5. **Present** — screenshot of Plasma shell (panel / wallpaper), not black Metal
6. **Store** — IPA embed + size gate; still Mode A jitless StaticCpu

## Risks

| Risk | Mitigation |
|------|------------|
| Plasma too heavy for StaticCpu | 2–4 GiB RAM; software compose; accept slow first paint |
| Rootfs size vs IPA / builder TMPDIR | Curated plasma-workspace (not desktopManager.plasma6); English locales only; optional later Apple ODR |
| KWin refuses QPainter nested | Fall back session: `kwin_wayland --platform wayland` then add plasmashell |
| Tip links stale `libwawona.a` | Rebuild `.#wawona-ios-sim-backend` with Terminal tip before claiming Terminal green |
| Eth0 / DHCP stalls | Keep `wawona-slirp-dns`; no dhcpcd; no `network-addresses-eth0` |

## Non-goals

- Full SDDM greeter inside the guest
- Guest GPU / Vulkan on Mode A
- Separate “download Plasma” button
- UTM / QEMU product path
- Claiming store TF green before embed + present evidence

## Related

- `wawona-relay-wasm` / `wawona-linux-vms-relay-runtime` / `wawona-guest-wayland-iland`
- Tip SHM: `staticcpu-shm-tip.md`, `agentic-graph-staticcpu.md`
- Container spike (kwin only): `import/containers/spike/`
- Terminal wrap: `Terminal/src/screen.rs` (`wrap_cont`)

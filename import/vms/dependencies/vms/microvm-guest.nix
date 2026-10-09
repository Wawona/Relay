# wawona-microvm — a NixOS guest driven by microvm.nix under vfkit
# (Apple Virtualization.framework) on macOS. Guest definition for the
# Linux-first dogfood path: `nix run .#wawona-microvm-session` (Wawona flake)
# supervises bridge + vfkit. Product Machines engine remains Relay; this module
# is the shared guest Wayland/vsock shape.
#
# WHY microvm.nix + vfkit instead of a hand-rolled rootfs:
#   * vfkit IS Virtualization.framework (same tech as the in-app Swift launcher
#     in ./WawonaLinuxVZ.swift), but maintained and driven declaratively by
#     microvm.nix.
#   * `writableStoreOverlay` + a virtiofs read-only share of the host /nix/store
#     means the rootfs is a tiny writable overlay disk — NO `make-disk-image`,
#     so NO nested QEMU/KVM VM is needed to build it (that is what stalled the
#     make-ext4-fs guest on the VZ Linux builder). The guest closure is realized
#     by the aarch64-linux builder and shared straight into the VM.
#   * vsock plumbing, Rosetta, virtiofs shares and NAT networking are all handled
#     by the microvm module.
#
# vsock topology (vfkit default "listen" mode == guest->host):
#   guest:  waypipe --no-gpu --vsock -s <port> server -- <client>
#           (CID omitted => connect out to host CID 2 on <port>)
#   vfkit:  --device virtio-vsock,port=<port>,socketURL=<unix sock> (listen):
#           when the guest connects to vsock <port>, vfkit connects to the host
#           unix socket, which the host bridge is LISTENING on.
#   host :  socat UNIX-LISTEN:<unix sock> -> waypipe client -> Wawona wayland-0
#
# NOTE: the vfkit runner hardcodes vsock port 1024, so `vsockPort` must stay 1024
# unless microvm.nix's runner gains a configurable port.
{
  nixpkgs,
  microvm,
  # aarch64-linux guest boots natively under VZ on Apple Silicon.
  guestSystem ? "aarch64-linux",
  # nixpkgs set that provides the vfkit/socat host tools (the Mac).
  hostSystem ? "aarch64-darwin",
  # vfkit hardcodes vsock port 1024; the guest waypipe server binds it.
  vsockPort ? 1024,
  # Host-side unix socket vfkit exposes for the guest vsock channel. The bridge
  # (wawona-vm-bridge / wawona-microvm-session) listens here.
  vsockSocketPath ? "/tmp/wawona-guest-vsock.sock",
  # Default Wayland *client* forwarded into Wawona. Override with a full argv
  # string (e.g. weston-terminal) or replace the unit via `extraModule`.
  sessionClient ? null,
  # Extra NixOS module to swap the session (wwn-niri/sway/hyprland/...).
  extraModule ? { },
}:

nixpkgs.lib.nixosSystem {
  modules = [
    microvm.nixosModules.microvm
    (
      { config, pkgs, lib, ... }:
      let
        # Default: several software Wayland clients share one waypipe display so
        # Linux-first dogfood exercises more than a single foot window. Override
        # with `sessionClient` for a single binary, or `extraModule` for a DE.
        multiClientLauncher =
          let
            W = "${pkgs.weston}/bin";
          in
          pkgs.writeShellScript "wawona-session-clients" ''
            set -eu
            echo "[wawona-session] WAYLAND_DISPLAY=''${WAYLAND_DISPLAY:-unset}" >&2
            echo "[wawona-session] launching multi-client set into Wawona" >&2
            pids=""
            launch() {
              local name="$1"; shift
              local bin="$1"
              shift || true
              if [ ! -x "$bin" ]; then
                echo "[wawona-session] skip $name (missing $bin)" >&2
                return 0
              fi
              echo "[wawona-session] start $name: $bin $*" >&2
              "$bin" "$@" &
              pids="$pids $!"
              # Stagger so waypipe + host compositor can accept each toplevel.
              sleep 1
            }
            # Software / SHM clients only. Skip EGL (subsurfaces),
            # wp_presentation v2 (presentation-shm), and nested sway for the
            # first dogfood set (sway needs a stable wl_compositor bind).
            launch foot ${pkgs.foot}/bin/foot
            launch weston-terminal ${W}/weston-terminal
            launch weston-flower ${W}/weston-flower
            launch weston-smoke ${W}/weston-smoke
            launch weston-clickdot ${W}/weston-clickdot
            launch weston-dnd ${W}/weston-dnd
            launch weston-editor ${W}/weston-editor
            launch weston-stacking ${W}/weston-stacking
            launch weston-transformed ${W}/weston-transformed
            launch weston-resizor ${W}/weston-resizor
            launch weston-scaler ${W}/weston-scaler
            launch weston-multi-resource ${W}/weston-multi-resource
            # Keep the waypipe server child alive while any client runs.
            status=0
            for pid in $pids; do
              wait "$pid" || status=$?
            done
            exit "$status"
          '';
        client =
          if sessionClient != null then sessionClient else "${multiClientLauncher}";
      in
      {
        nixpkgs.hostPlatform = guestSystem;

        networking.hostName = "wawona-guest";
        system.stateVersion = "24.11";

        users.users.wawona = {
          isNormalUser = true;
          initialPassword = "wawona";
          extraGroups = [ "wheel" "video" "input" ];
        };
        services.getty.autologinUser = "wawona";
        security.sudo.wheelNeedsPassword = false;

        microvm = {
          hypervisor = "vfkit";
          vcpu = 4;
          mem = 4096;
          # Host tools (vfkit, socat) come from the Mac's nixpkgs.
          vmHostPackages = nixpkgs.legacyPackages.${hostSystem};
          # Share the host store read-only and layer a writable overlay on top —
          # no full-closure rootfs image, no KVM required to build it.
          writableStoreOverlay = "/nix/.rw-store";
          shares = [
            {
              proto = "virtiofs";
              tag = "ro-store";
              source = "/nix/store";
              mountPoint = "/nix/.ro-store";
            }
          ];
          volumes = [
            {
              image = "wawona-microvm.img";
              mountPoint = "/nix/.rw-store";
              size = 10240;
            }
          ];
          interfaces = [
            {
              type = "user";
              id = "usernet";
              mac = "02:00:00:0a:0b:0c";
            }
          ];
          # Enables graceful `{"state":"Stop"}` shutdown over the restful socket.
          socket = "wawona-microvm.sock";
          # Attach the virtio-vsock device via extraArgs rather than `vsock.cid`:
          # upstream microvm.nix's vfkit runner still throws on `vsock.cid != null`
          # ("vfkit vsock support not yet implemented"), but appends extraArgs
          # verbatim. This keeps Wawona on upstream microvm.nix (no fork/patch)
          # while still giving the guest the vsock channel the Wayland bridge needs.
          # The guest connects to host CID 2 on this port; vfkit relays it to
          # ${vsockSocketPath} on the Mac.
          vfkit.extraArgs = [
            "--device"
            "virtio-vsock,port=${toString vsockPort},socketURL=${vsockSocketPath}"
          ];
        };

        networking.interfaces.eth0.useDHCP = true;

        # Nix-in-guest builds must not fill the tmpfs root (they go on the overlay
        # disk). See the microvm.nix macOS gotcha.
        systemd.tmpfiles.rules = [ "d /nix/.rw-store/nix-build 0755 root root -" ];
        nix.settings = {
          sandbox = false;
          build-dir = "/nix/.rw-store/nix-build";
          experimental-features = [ "nix-command" "flakes" ];
        };

        # Software rendering — vfkit has no GPU passthrough for wlroots here.
        environment.variables = {
          WLR_RENDERER = "pixman";
          WLR_NO_HARDWARE_CURSORS = "1";
        };

        environment.systemPackages = with pkgs; [
          waypipe
          sway
          foot
          weston
          wayland-utils
          git
          vim
        ];

        # Auto-forward the guest Wayland session to the host on boot: waypipe
        # server connects out to the host over vsock <vsockPort>; the host bridge
        # relays it into Wawona, which IS the compositor.
        #
        # IMPORTANT: waypipe forwards Wayland *clients*, not compositors. Wawona is
        # the compositor. Default session launches foot + weston-terminal +
        # weston-simple-shm (+ optional nested sway on WLR_BACKENDS=wayland).
        # Swap via `sessionClient` or `extraModule`.
        systemd.services.wawona-session = {
          description = "Wawona Wayland session forwarded to host over vsock";
          wantedBy = [ "multi-user.target" ];
          after = [ "network.target" ];
          serviceConfig = {
            User = "wawona";
            PAMName = "login";
            WorkingDirectory = "/home/wawona";
            TTYPath = "/dev/tty7";
            Restart = "always";
            RestartSec = "2s";
            # Surface waypipe's logs on the guest console (hvc0) so the host
            # launcher's captured console shows the vsock handshake / errors.
            StandardOutput = "journal+console";
            StandardError = "journal+console";
          };
          environment = {
            XDG_RUNTIME_DIR = "/run/user/1000";
          };
          script = ''
            mkdir -p "$XDG_RUNTIME_DIR"
            echo "[wawona-session] waypipe $(${pkgs.waypipe}/bin/waypipe --version 2>&1 | head -1)" >&2
            echo "[wawona-session] connecting waypipe server to host vsock CID 2 port ${toString vsockPort}" >&2
            # Ready marker for host scrapers (same string as Relay guest units).
            # Printed before dial-out so session supervision can proceed while
            # waypipe retries vsock until the host bridge is listening.
            printf 'WAWONA_RELAY_READY=1\n' >&2
            if [ -w /dev/hvc0 ]; then
              printf 'WAWONA_RELAY_READY=1\n' > /dev/hvc0 || true
            fi
            # waypipe's guest->host vsock form: `--vsock -s <port> server` with the
            # CID omitted connects out to the host (CID 2). vfkit (default listen
            # mode) forwards that to the host-side unix socket. The forwarded
            # command must be a Wayland *client*, whose window appears in Wawona.
            # Set WAYPIPE_DEBUG=1 (via the unit env) to add --debug for tracing.
            exec ${pkgs.waypipe}/bin/waypipe \
              ''${WAYPIPE_DEBUG:+--debug} \
              --no-gpu \
              --vsock -s ${toString vsockPort} \
              server -- ${client}
          '';
        };

        documentation.enable = false;
        documentation.nixos.enable = false;
        documentation.man.enable = false;
      }
    )
    extraModule
  ];
}

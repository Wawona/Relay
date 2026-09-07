# wawona-microvm — a NixOS guest driven by microvm.nix under vfkit
# (Apple Virtualization.framework) on macOS. This is the p26 "NixOS VM" machine
# type's *developer path*: `nix run .#wawona-microvm` builds+boots the guest and
# `nix run .#wawona-vm-bridge` forwards its Wayland session into Wawona.
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
#   guest:  waypipe --vsock -s <port> server -- sway   (CID omitted => connect
#           out to host CID 2 on <port>; this is waypipe's documented guest->host
#           form, NOT `--socket vsock:2:<port>` which is a literal unix path)
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
  # (wawona-vm-bridge) connects here and relays into Wawona's wayland-0.
  vsockSocketPath ? "/tmp/wawona-guest-vsock.sock",
  # Extra NixOS module to swap the session (wwn-niri/sway/hyprland/...).
  extraModule ? { },
}:

nixpkgs.lib.nixosSystem {
  modules = [
    microvm.nixosModules.microvm
    (
      { config, pkgs, lib, ... }:
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
          wayland-utils
          git
          vim
        ];

        # Auto-forward the guest Wayland session to the host on boot: waypipe
        # server connects out to the host over vsock <vsockPort>; the host bridge
        # relays it into Wawona, which IS the compositor.
        #
        # IMPORTANT: waypipe forwards Wayland *clients*, not compositors. Wawona is
        # the compositor, so the guest runs a client app (foot) whose window
        # appears as a native Wawona window. Running a nested compositor here would
        # require it to act as a Wayland *client* of waypipe's display (e.g. sway
        # with WLR_BACKENDS=wayland) — that is the Phase-29 wwn-* nested-compositor
        # path; for the base p26 machine we forward a client directly.
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
            # waypipe's guest->host vsock form: `--vsock -s <port> server` with the
            # CID omitted connects out to the host (CID 2). vfkit (default listen
            # mode) forwards that to the host-side unix socket. The forwarded
            # command must be a Wayland *client* (foot), whose window appears in
            # Wawona as a native window.
            # Set WAYPIPE_DEBUG=1 (via the unit env) to add --debug for tracing.
            exec ${pkgs.waypipe}/bin/waypipe \
              ''${WAYPIPE_DEBUG:+--debug} \
              --vsock -s ${toString vsockPort} \
              server -- ${pkgs.foot}/bin/foot
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

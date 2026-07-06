# Minimal NixOS aarch64-linux guest for the mobile QEMU-TCTI engine
# (iOS/iPadOS/visionOS/tvOS). This is the "guest is data" artifact from
# COMPLIANCE.md: kernel + rootfs are bundled / On-Demand-Resource data, never
# downloaded executable code.
#
# Kept deliberately tiny (tight mobile RAM ceiling, no JIT in the guest either):
# a headless wlroots session (cage + foot) whose pixels are streamed to Wawona
# by waypipe over vsock rather than scanned out to an emulated framebuffer.
#
# Evaluates on any host; the kernel/rootfs artifacts build on the aarch64-linux
# builder (Determinate native Linux builder on macOS). The QEMU-TCTI *engine*
# that boots this guest is engine.nix (built from the vendored UTM sources in ../utm).
{
  nixpkgs,
  guestSystem ? "aarch64-linux",
  # vsock port the guest's waypipe server binds; the host engine relays it into
  # Wawona (matches the macOS microvm/vz topology).
  vsockPort ? 1024,
  extraModule ? { },
}:

nixpkgs.lib.nixosSystem {
  system = guestSystem;
  modules = [
    (
      { config, pkgs, lib, ... }:
      {
        nixpkgs.hostPlatform = guestSystem;
        networking.hostName = "wawona-mobile-guest";
        system.stateVersion = "24.11";

        # Modern systemd initrd (scripted initrd is deprecated).
        boot.initrd.systemd.enable = true;
        boot.loader.grub.enable = false;
        boot.kernelParams = [ "console=hvc0" "quiet" ];
        # QEMU-TCTI boots the ext4 rootfs off virtio-blk (/dev/vda); the engine
        # passes the kernel + this rootfs directly (no bootloader).
        boot.initrd.availableKernelModules = [ "virtio_blk" "virtio_pci" "virtio_console" ];
        fileSystems."/" = {
          device = "/dev/vda";
          fsType = "ext4";
          autoResize = true;
        };

        users.users.wawona = {
          isNormalUser = true;
          initialPassword = "wawona";
          extraGroups = [ "wheel" "video" "input" ];
        };
        services.getty.autologinUser = "wawona";
        security.sudo.wheelNeedsPassword = false;

        # Software rendering only — no GPU passthrough under QEMU-TCTI.
        environment.variables = {
          WLR_RENDERER = "pixman";
          WLR_NO_HARDWARE_CURSORS = "1";
        };

        environment.systemPackages = with pkgs; [ waypipe cage foot wayland-utils ];

        # Headless Wayland session forwarded to the host over vsock on boot.
        systemd.services.wawona-session = {
          description = "Wawona mobile Wayland session forwarded over vsock";
          wantedBy = [ "multi-user.target" ];
          after = [ "systemd-user-sessions.service" ];
          serviceConfig = {
            User = "wawona";
            WorkingDirectory = "/home/wawona";
            Restart = "always";
            RestartSec = "2s";
          };
          environment = {
            XDG_RUNTIME_DIR = "/run/user/1000";
            WLR_BACKENDS = "headless";
            WLR_RENDERER = "pixman";
            WLR_NO_HARDWARE_CURSORS = "1";
          };
          script = ''
            mkdir -p "$XDG_RUNTIME_DIR"
            exec ${pkgs.waypipe}/bin/waypipe \
              --socket vsock:2:${toString vsockPort} \
              server -- ${pkgs.cage}/bin/cage -- ${pkgs.foot}/bin/foot
          '';
        };

        # Trim the closure hard for the mobile RAM/space ceiling.
        documentation.enable = false;
        documentation.nixos.enable = false;
        documentation.man.enable = false;
        services.udisks2.enable = false;
        fonts.fontconfig.enable = lib.mkDefault true;
      }
    )
    extraModule
  ];
}

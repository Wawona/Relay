# User-owned guest configuration. Keep ./relay.nix imported for Wawona devices
# and transport, then customize packages and the Wayland desktop here.
#
# Mode A product session: KDE Plasma 6 Wayland over waypipe --no-gpu (SHM).
# Nested KWin paints into Wawona Present. Do not expect a guest GPU ICD.
{ pkgs, lib, ... }:
{
  imports = [ ./relay.nix ];
  networking.hostName = "wawona-mobile-guest";
  system.stateVersion = "24.11";

  # Plasma 6 packages + session helpers. No SDDM: Relay's wawona-session
  # unit launches startplasma-wayland under waypipe (same vsock path as
  # the prior weston-smoke SHM proof).
  services.desktopManager.plasma6.enable = true;
  # relay.nix keeps the guest slim (udisks2 off). Plasma6 flips it on;
  # force slim for StaticCpu boot time. Removable media is not the goal.
  services.udisks2.enable = lib.mkForce false;

  # weston-smoke stays packaged for lab SHM bring-up; session is Plasma.
  nixpkgs.overlays = [
    (final: prev: {
      weston = prev.weston.overrideAttrs (old: {
        postPatch = (old.postPatch or "") + ''
          substituteInPlace clients/smoke.c \
            --replace-fail 'smoke.width = 200;' 'smoke.width = 800;' \
            --replace-fail 'smoke.height = 200;' 'smoke.height = 600;'
        '';
      });
    })
  ];

  environment.systemPackages = with pkgs; [
    cage
    foot
    wayland-utils
    weston
    kdePackages.qtwayland
  ];

  # StaticCpu / waypipe --no-gpu: force software compose. Guest has no
  # Vulkan/EGL ICD; Cage previously died on swapchain format for the same
  # reason. QPainter KWin is the nested SHM path (spike session.sh kwin mode).
  environment.variables = {
    KWIN_COMPOSE = "QPainter";
    QT_QUICK_BACKEND = "software";
    LIBGL_ALWAYS_SOFTWARE = "1";
    QT_QPA_PLATFORM = "wayland";
  };

  wawona.relay.sessionCommand = [
    "${pkgs.dbus}/bin/dbus-run-session"
    "--"
    "${pkgs.kdePackages.plasma-workspace}/bin/startplasma-wayland"
  ];
}

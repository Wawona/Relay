# User-owned guest configuration. Keep ./relay.nix imported for Wawona devices
# and transport, then customize packages and the Wayland desktop here.
#
# Mode A Present session: KWin Wayland (QPainter) over waypipe --no-gpu (SHM).
# Nested KWin paints into Wawona Present. Do not expect a guest GPU ICD.
#
# Size gates (measured 2026-11-02):
# - services.desktopManager.plasma6.enable → ~7 GiB closure
# - curated plasma-workspace + kwin + breeze → ~4.1 GiB closure
# - kwin alone → ~2.0 GiB closure
# make-ext4-fs stages a full copy into builder TMPDIR (~2× closure). The
# Determinated Linux builder overflows above ~3 GiB closure. Ship KWin first;
# startplasma-wayland returns once the builder disk / ODR path lands.
{ pkgs, lib, ... }:
{
  imports = [ ./relay.nix ];
  networking.hostName = "wawona-mobile-guest";
  system.stateVersion = "24.11";

  i18n.defaultLocale = "en_US.UTF-8";
  i18n.supportedLocales = [
    "C.UTF-8/UTF-8"
    "en_US.UTF-8/UTF-8"
  ];

  # No SDDM. Relay's wawona-session launches the session under waypipe.
  services.udisks2.enable = lib.mkForce false;

  # weston-smoke stays packaged for lab SHM bring-up.
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
    foot
    wayland-utils
    weston
    kdePackages.qtwayland
    kdePackages.kwin
  ];

  # StaticCpu / waypipe --no-gpu: force software compose.
  environment.variables = {
    KWIN_COMPOSE = "QPainter";
    QT_QUICK_BACKEND = "software";
    LIBGL_ALWAYS_SOFTWARE = "1";
    QT_QPA_PLATFORM = "wayland";
  };

  # KWin as the nested compositor. Foot proves a client surface on Present.
  # Replace with startplasma-wayland when the guest image budget allows it.
  wawona.relay.sessionCommand = [
    "${pkgs.dbus}/bin/dbus-run-session"
    "--"
    "${pkgs.kdePackages.kwin}/bin/kwin_wayland"
    "--xwayland"
    "--exit-with-session=${pkgs.foot}/bin/foot"
  ];
}

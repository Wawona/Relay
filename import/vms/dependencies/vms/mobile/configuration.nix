# User-owned guest configuration. Keep ./relay.nix imported for Wawona devices
# and transport, then customize packages and the Wayland desktop here.
{ pkgs, ... }:
{
  imports = [ ./relay.nix ];
  networking.hostName = "wawona-mobile-guest";
  system.stateVersion = "24.11";
  environment.systemPackages = with pkgs; [ cage foot wayland-utils weston ];
  # Cage+foot needs a full nested compositor. Mode A SHM acceptance uses
  # weston-simple-shm over waypipe --no-gpu (recognizable face pattern).
  # weston-presentation-shm paints a timing field that looks like noise on
  # Metal letterbox; do not use it as the store-shaped present proof client.
  # A cage swapchain miss is not a frame.
  wawona.relay.sessionCommand = [
    "${pkgs.weston}/bin/weston-simple-shm"
  ];
}

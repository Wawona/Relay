# User-owned guest configuration. Keep ./relay.nix imported for Wawona devices
# and transport, then customize packages and the Wayland desktop here.
{ pkgs, ... }:
{
  imports = [ ./relay.nix ];
  networking.hostName = "wawona-mobile-guest";
  system.stateVersion = "24.11";
  environment.systemPackages = with pkgs; [ cage foot wayland-utils weston ];
  # Cage+foot needs a full nested compositor. The SHM acceptance frame is
  # weston-simple-shm over waypipe --no-gpu. A cage swapchain miss is not
  # a frame.
  wawona.relay.sessionCommand = [
    "${pkgs.weston}/bin/weston-presentation-shm"
  ];
}

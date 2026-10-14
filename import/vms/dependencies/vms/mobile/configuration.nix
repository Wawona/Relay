# User-owned guest configuration. Keep ./relay.nix imported for Wawona devices
# and transport, then customize packages and the Wayland desktop here.
{ pkgs, ... }:
{
  imports = [ ./relay.nix ];
  networking.hostName = "wawona-mobile-guest";
  system.stateVersion = "24.11";
  environment.systemPackages = with pkgs; [ cage foot wayland-utils weston ];
  # Cage+foot needs a full nested compositor. Mode A SHM acceptance uses a
  # CPU SHM client over waypipe --no-gpu.
  # nixpkgs weston 16.0.0 no longer ships weston-simple-shm (path was a
  # string-only store join; unit failed ENOENT and blocked AUTH). weston-smoke
  # is still in the package and speaks wl_shm. Do not use
  # weston-presentation-shm as the store-shaped present proof (timing field
  # looks like noise on Metal letterbox). A cage swapchain miss is not a frame.
  wawona.relay.sessionCommand = [
    "${pkgs.weston}/bin/weston-smoke"
  ];
}

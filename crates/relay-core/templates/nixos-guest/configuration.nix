# User-owned guest configuration. Keep ./relay.nix imported for Wawona devices
# and transport, then customize packages and the Wayland desktop here.
{ pkgs, ... }:
{
  imports = [ ./relay.nix ];
  networking.hostName = "wawona-mobile-guest";
  system.stateVersion = "24.11";
  environment.systemPackages = with pkgs; [ cage foot wayland-utils ];
  wawona.relay.sessionCommand = [
    "${pkgs.cage}/bin/cage" "--" "${pkgs.foot}/bin/foot"
  ];
}

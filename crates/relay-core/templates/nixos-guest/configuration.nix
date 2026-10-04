# Customize packages and the Wayland desktop. relay.nix enables flakes and
# nix-command and supplies Wawona's hardware and transport integration.
{ pkgs, ... }:
{
  imports = [ ./relay.nix ];
  networking.hostName = "wawona";
  system.stateVersion = "24.11";
  environment.systemPackages = with pkgs; [ cage foot wayland-utils ];
  wawona.relay.sessionCommand = [
    "${pkgs.cage}/bin/cage" "--" "${pkgs.foot}/bin/foot"
  ];
}

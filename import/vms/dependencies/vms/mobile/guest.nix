# NixOS guest evaluation entry point. Runtime integration and user configuration
# are separate modules; both page geometries use the same flake-ready guest.
{ nixpkgs, guestSystem ? "aarch64-linux", pageSize ? 4096,
  vsockPort ? 1024, extraModule ? { }, configurationModule ? ./configuration.nix }:
assert builtins.elem pageSize [ 4096 16384 ];
nixpkgs.lib.nixosSystem {
  system = guestSystem;
  specialArgs = {
    relayGuestSystem = guestSystem;
    relayPageSize = pageSize;
    relayVsockPort = vsockPort;
  };
  modules = [ configurationModule extraModule ];
}

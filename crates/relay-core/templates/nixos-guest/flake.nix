{
  description = "My Wawona Relay NixOS machine";
  inputs.relay.url = "github:Wawona/Relay/development";
  inputs.nixpkgs.follows = "relay/nixpkgs";
  outputs = { nixpkgs, relay, ... }: {
    nixosConfigurations.wawona = nixpkgs.lib.nixosSystem {
      system = "aarch64-linux";
      specialArgs.relayModule = relay.nixosModules.relay;
      modules = [ ./configuration.nix ];
    };
  };
}

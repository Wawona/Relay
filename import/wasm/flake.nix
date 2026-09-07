{
  description = "wwn-wasm: in-process WASI P1/P2 interpreter for Wawona (Pulley on Apple mobile).";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.url = "https://flakehub.com/f/Wawona/wwn-toolchain/*";
    wwn-toolchain.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.inputs.rust-overlay.follows = "rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay, wwn-toolchain, ... }:
    let
      darwinSystems = [ "aarch64-darwin" ];
      linuxSystems = [ "x86_64-linux" "aarch64-linux" ];
      allSystems = darwinSystems ++ linuxSystems;
      forAll = nixpkgs.lib.genAttrs allSystems;
      inherit (wwn-toolchain.lib) withPlatformVariants baseRegistry mkToolchains;

      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
        config = { allowUnfree = true; allowUnsupportedSystem = true; android_sdk.accept_license = true; };
      };

      wasmDir = ./dependencies/libs/wasm;
    in
    {
      registryFragment = {
        wawona-wasm = withPlatformVariants {
          android = wasmDir + "/android.nix";
          ios = wasmDir + "/ios.nix";
          tvos = wasmDir + "/tvos.nix";
          ipados = wasmDir + "/ipados.nix";
          visionos = wasmDir + "/visionos.nix";
          watchos = wasmDir + "/watchos.nix";
          macos = wasmDir + "/macos.nix";
          linux = wasmDir + "/linux.nix";
        };
      };

      packages = forAll (system:
        let
          pkgs = pkgsFor system;
          tc = mkToolchains { inherit pkgs; registry = baseRegistry // self.registryFragment; };
          isDarwin = builtins.elem system darwinSystems;
          host =
            if isDarwin then tc.buildForMacOS "wawona-wasm" { }
            else tc.buildForLinux "wawona-wasm" { };
        in
        {
          default = host;
          wawona-wasm = host;
        } // (if isDarwin then {
          wawona-wasm-macos = host;
          wawona-wasm-ios = tc.buildForIOS "wawona-wasm" { };
          wawona-wasm-watchos = tc.buildForWatchOS "wawona-wasm" { };
          wawona-wasm-watchos-sim = tc.buildForWatchOS "wawona-wasm" { simulator = true; };
        } else {
          wawona-wasm-linux = host;
        })
      );

      formatter = forAll (system: (pkgsFor system).nixfmt-rfc-style);
    };
}

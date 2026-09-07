{
  description = "Wawona Relay: Linux VMs, OCI-in-VM, Mode A WASI. No QEMU, no UTM.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.url = "github:Wawona/wwn-toolchain/development";
    wwn-toolchain.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.inputs.rust-overlay.follows = "rust-overlay";
    microvm.url = "github:microvm-nix/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay, wwn-toolchain, microvm, ... }:
    let
      darwinSystems = [ "aarch64-darwin" ];
      linuxSystems = [ "x86_64-linux" "aarch64-linux" ];
      allSystems = darwinSystems ++ linuxSystems;
      forAll = nixpkgs.lib.genAttrs allSystems;
      inherit (wwn-toolchain.lib) withPlatformVariants baseRegistry mkToolchains;

      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
        config = {
          allowUnfree = true;
          allowUnsupportedSystem = true;
          android_sdk.accept_license = true;
        };
      };

      wasmDir = ./import/wasm/dependencies/libs/wasm;
      vmsDir = ./import/vms/dependencies/vms;
      containersDir = ./import/containers/dependencies/containers;
      failClosed = ./recipes/vm-engine-fail-closed.nix;
    in
    {
      # Re-export keys L4 already consumes (wawona-wasm, nixos-vm, vm-engine,
      # oci-*). vm-engine is VZ / KVM / fail-closed. Never QEMU.
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
        nixos-vm = withPlatformVariants {
          macos = vmsDir + "/microvm-guest.nix";
          ios = vmsDir + "/mobile/guest.nix";
          ipados = vmsDir + "/mobile/guest.nix";
          tvos = vmsDir + "/stub.nix";
          visionos = vmsDir + "/stub.nix";
          watchos = vmsDir + "/stub.nix";
          android = vmsDir + "/mobile/guest.nix";
          wearos = vmsDir + "/stub.nix";
        };
        vm-engine = withPlatformVariants {
          macos = ./recipes/vm-engine-macos-vz.nix;
          linux = ./recipes/vm-engine-linux-kvm.nix;
          ios = failClosed;
          ipados = failClosed;
          tvos = vmsDir + "/stub.nix";
          visionos = vmsDir + "/stub.nix";
          watchos = vmsDir + "/stub.nix";
          android = failClosed;
          wearos = vmsDir + "/stub.nix";
        };
        vm-engine-contract = withPlatformVariants {
          ios = failClosed;
          ipados = failClosed;
        };
        vm-engine-contract-modeb = withPlatformVariants {
          ios = failClosed;
          ipados = failClosed;
        };
        vm-engine-jit = withPlatformVariants {
          ios = failClosed;
          ipados = failClosed;
        };
        oci-image = withPlatformVariants {
          macos = containersDir + "/registry/oci-image.nix";
          ios = containersDir + "/registry/oci-image.nix";
          ipados = containersDir + "/registry/oci-image.nix";
          tvos = containersDir + "/registry/oci-image.nix";
          visionos = containersDir + "/registry/oci-image.nix";
          watchos = containersDir + "/registry/oci-image.nix";
          android = containersDir + "/registry/oci-image.nix";
          wearos = containersDir + "/registry/oci-image.nix";
        };
        oci-runtime = withPlatformVariants {
          macos = containersDir + "/registry/oci-runtime-macos.nix";
          ios = containersDir + "/registry/oci-runtime-mobile.nix";
          ipados = containersDir + "/registry/oci-runtime-mobile.nix";
          tvos = containersDir + "/registry/oci-runtime-mobile.nix";
          visionos = containersDir + "/registry/oci-runtime-mobile.nix";
          watchos = containersDir + "/registry/oci-runtime-image-only.nix";
          android = containersDir + "/registry/oci-runtime-android.nix";
          wearos = containersDir + "/registry/oci-runtime-image-only.nix";
        };
        container-cli = withPlatformVariants {
          macos = containersDir + "/registry/container-cli.nix";
          ios = containersDir + "/registry/container-cli.nix";
          ipados = containersDir + "/registry/container-cli.nix";
          tvos = containersDir + "/registry/container-cli.nix";
          visionos = containersDir + "/registry/container-cli.nix";
          watchos = containersDir + "/registry/container-cli.nix";
          android = containersDir + "/registry/container-cli.nix";
          wearos = containersDir + "/registry/container-cli.nix";
        };
        apple-container = withPlatformVariants {
          macos = containersDir + "/registry/apple-container.nix";
          ios = containersDir + "/macos/apple-container-forbidden.nix";
          ipados = containersDir + "/macos/apple-container-forbidden.nix";
          tvos = containersDir + "/macos/apple-container-forbidden.nix";
          watchos = containersDir + "/macos/apple-container-forbidden.nix";
          visionos = containersDir + "/macos/apple-container-forbidden.nix";
          android = containersDir + "/macos/apple-container-forbidden.nix";
          linux = containersDir + "/macos/apple-container-forbidden.nix";
        };
        wawona-relay = withPlatformVariants {
          macos = ./recipes/relay-staticlib.nix;
          linux = ./recipes/relay-staticlib.nix;
          ios = ./recipes/relay-staticlib.nix;
          ipados = ./recipes/relay-staticlib.nix;
          tvos = ./recipes/relay-staticlib.nix;
          watchos = ./recipes/relay-staticlib.nix;
          visionos = ./recipes/relay-staticlib.nix;
          android = ./recipes/relay-staticlib.nix;
        };
      };

      packages = forAll (system:
        let
          pkgs = pkgsFor system;
          tc = mkToolchains { inherit pkgs; registry = baseRegistry // self.registryFragment; };
          isDarwin = builtins.elem system darwinSystems;
          hostWasm =
            if isDarwin then tc.buildForMacOS "wawona-wasm" { }
            else tc.buildForLinux "wawona-wasm" { };
          hostRelay = pkgs.callPackage ./recipes/relay-staticlib.nix { };
        in
        {
          default = hostRelay;
          wawona-relay = hostRelay;
          wawona-wasm = hostWasm;
        } // (if isDarwin then {
          wawona-wasm-macos = hostWasm;
          wawona-wasm-ios = tc.buildForIOS "wawona-wasm" { };
          wawona-wasm-watchos = tc.buildForWatchOS "wawona-wasm" { };
          wawona-wasm-watchos-sim = tc.buildForWatchOS "wawona-wasm" { simulator = true; };
        } else {
          wawona-wasm-linux = hostWasm;
        })
      );

      inherit microvm;

      formatter = forAll (system: (pkgsFor system).nixfmt-rfc-style);
    };
}

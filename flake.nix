{
  description = "Wawona Relay: Linux VMs, OCI-in-VM, Mode A WASI. No QEMU, no UTM.";

  # Repo ownership remains L3 prime as defined by
  # Wawona/docs/wwn-repo-dag.md. NixOS and microvm.nix are upstream inputs,
  # never Wawona L4 or a graphics consumer edge.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.url = "github:Wawona/wwn-toolchain/development";
    wwn-toolchain.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.inputs.rust-overlay.follows = "rust-overlay";
    # Per-crate Nix store builds. Pin follows L0 wwn-toolchain (not a second
    # independent crate2nix tip). Product crates must migrate off monolithic
    # buildRustPackage onto generatedCargoNix.
    crate2nix.follows = "wwn-toolchain/crate2nix";
    microvm.url = "github:microvm-nix/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      wwn-toolchain,
      crate2nix,
      microvm,
      ...
    }:
    let
      darwinSystems = [ "aarch64-darwin" ];
      linuxSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      allSystems = darwinSystems ++ linuxSystems;
      forAll = nixpkgs.lib.genAttrs allSystems;
      inherit (wwn-toolchain.lib) withPlatformVariants baseRegistry mkToolchains;

      pkgsFor =
        system:
        import nixpkgs {
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
          macos = ./recipes/relay-crate2nix.nix;
          linux = ./recipes/relay-crate2nix.nix;
          ios = ./recipes/relay-staticlib.nix;
          ipados = ./recipes/relay-staticlib.nix;
          tvos = ./recipes/relay-staticlib.nix;
          watchos = ./recipes/relay-staticlib.nix;
          visionos = ./recipes/relay-staticlib.nix;
          android = ./recipes/relay-staticlib.nix;
        };
      };

      packages = forAll (
        system:
        let
          pkgs = pkgsFor system;
          tc = mkToolchains {
            inherit pkgs;
            registry = baseRegistry // self.registryFragment;
          };
          isDarwin = builtins.elem system darwinSystems;
          hostWasm =
            if isDarwin then tc.buildForMacOS "wawona-wasm" { } else tc.buildForLinux "wawona-wasm" { };
          hostRelay = pkgs.callPackage ./recipes/relay-crate2nix.nix {
            inherit crate2nix;
          };
          hostRelayStatic = pkgs.callPackage ./recipes/relay-staticlib.nix { };
          guestPkgs = pkgsFor "aarch64-linux";
          mobileGuest4kConfig = import ./import/vms/dependencies/vms/mobile/guest.nix {
            inherit nixpkgs;
            guestSystem = "aarch64-linux";
            pageSize = 4096;
          };
          mobileGuest4k = guestPkgs.callPackage ./import/vms/dependencies/vms/mobile/guest-artifacts.nix {
            mobileGuest = mobileGuest4kConfig;
            pageSize = 4096;
          };
          mobileGuest16kConfig = import ./import/vms/dependencies/vms/mobile/guest.nix {
            inherit nixpkgs;
            guestSystem = "aarch64-linux";
            pageSize = 16384;
          };
          mobileGuest16k = guestPkgs.callPackage ./import/vms/dependencies/vms/mobile/guest-artifacts.nix {
            mobileGuest = mobileGuest16kConfig;
            pageSize = 16384;
          };
        in
        {
          default = hostRelay;
          wawona-relay = hostRelay;
          wawona-relay-staticlib = hostRelayStatic;
          wawona-wasm = hostWasm;
          wawona-nixos-guest-4k = mobileGuest4k;
          wawona-nixos-guest-16k = mobileGuest16k;
          relay-mode-a-bench = pkgs.callPackage ./recipes/relay-mode-a-bench.nix {
            inherit crate2nix;
          };
        }
        // (
          if isDarwin then
            {
              wawona-vz-run = pkgs.callPackage ./import/vms/dependencies/vms/vz-launcher.nix { };
              wawona-wasm-macos = hostWasm;
              wawona-wasm-ios = tc.buildForIOS "wawona-wasm" { };
              wawona-wasm-watchos = tc.buildForWatchOS "wawona-wasm" { };
              wawona-wasm-watchos-sim = tc.buildForWatchOS "wawona-wasm" { simulator = true; };
              wawona-relay-ios = tc.buildForIOS "wawona-relay" { };
              wawona-relay-ios-sim = tc.buildForIOS "wawona-relay" { simulator = true; };
              wawona-relay-watchos = tc.buildForWatchOS "wawona-relay" { };
              wawona-relay-watchos-sim = tc.buildForWatchOS "wawona-relay" { simulator = true; };
            }
          else
            {
              wawona-wasm-linux = hostWasm;
            }
        )
      );

      apps = forAll (
        system:
        let
          pkg = self.packages.${system}.relay-mode-a-bench;
        in
        {
          relay-mode-a-bench = {
            type = "app";
            program = "${pkg}/bin/relay-mode-a-bench";
          };
        }
      );

      inherit microvm;

      formatter = forAll (system: (pkgsFor system).nixfmt-rfc-style);
    };
}

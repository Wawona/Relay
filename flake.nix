{
  description = "wwn-vms: Wawona's virtual-machine substrate. NixOS-only built-in guest, one engine per target: Virtualization.framework (microvm.nix + vfkit) on macOS; jitless QEMU-TCTI (UTM SE model) on iOS/iPadOS/tvOS/visionOS; QEMU/AVF on Android. SKELETON - real per-target engines are downstream (see README.md, COMPLIANCE.md).";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.url = "github:Wawona/wwn-toolchain";
    wwn-toolchain.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.inputs.rust-overlay.follows = "rust-overlay";
    # The macOS built-in NixOS VM path (relocated from Wawona) uses microvm.nix
    # to build the guest + drive vfkit (Virtualization.framework).
    microvm.url = "github:microvm-nix/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay, wwn-toolchain, microvm, ... }:
    let
      darwinSystems = [ "x86_64-darwin" "aarch64-darwin" ];
      linuxSystems = [ "x86_64-linux" "aarch64-linux" ];
      allSystems = darwinSystems ++ linuxSystems;
      forAll = nixpkgs.lib.genAttrs allSystems;
      inherit (wwn-toolchain.lib) withPlatformVariants;

      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
        config = { allowUnfree = true; allowUnsupportedSystem = true; android_sdk.accept_license = true; };
      };

      dir = ./dependencies/vms;
    in
    {
      # Registry fragment merged into Wawona's client/machine registry. Entries
      # currently point at stubs that evaluate cleanly (so the registry merges and
      # CI can enumerate targets) but fail the build with a clear message until the
      # per-target engine lands.
      #
      #   nixos-vm     the NixOS-only built-in VM (the whole point of wwn-vms)
      #   vm-engine    the per-target hypervisor/emulator backend
      registryFragment = {
        nixos-vm = withPlatformVariants {
          macos = dir + "/stub.nix";
          ios = dir + "/stub.nix";
          ipados = dir + "/stub.nix";
          tvos = dir + "/stub.nix";
          visionos = dir + "/stub.nix";
          watchos = dir + "/stub.nix";
          android = dir + "/stub.nix";
          wearos = dir + "/stub.nix";
        };
        vm-engine = withPlatformVariants {
          macos = dir + "/stub.nix";
          ios = dir + "/stub.nix";
          ipados = dir + "/stub.nix";
          tvos = dir + "/stub.nix";
          visionos = dir + "/stub.nix";
          watchos = dir + "/stub.nix";
          android = dir + "/stub.nix";
          wearos = dir + "/stub.nix";
        };
      };

      # Vendored UTM engine sources (QEMU-TCTI patches + build machinery + VZ
      # reference backends) — formerly the separate `wwn-utm` repo, now folded
      # in-repo so wwn-vms is self-contained (see dependencies/vms/utm/README.md).
      # mobile/engine.nix + android/engine.nix consume these paths via their
      # `utm` argument.
      lib.utm = {
        dir = dir + "/utm";
        qemuUtmPatch = dir + "/utm/patches/qemu-10.0.2-utm.patch";
        patchesDir = dir + "/utm/patches";
        buildDependenciesScript = dir + "/utm/scripts/build_dependencies.sh";
        packDependenciesScript = dir + "/utm/scripts/pack_dependencies.sh";
        sources = {
          qemuSystem = dir + "/utm/sources/UTMQemuSystem.m";
          qemuVirtualMachine = dir + "/utm/sources/UTMQemuVirtualMachine.swift";
          appleVirtualMachine = dir + "/utm/sources/UTMAppleVirtualMachine.swift";
          qemuProcess = dir + "/utm/sources/UTMProcess.m";
        };
        seScheme = dir + "/utm/xcschemes/iOS-SE.xcscheme";
      };

      # Per-target VM capability matrix (with eval-time invariant asserts).
      # `nix eval .#lib.capabilities` is the VM capability-lane gate.
      lib.capabilities = import ./capabilities.nix;

      # Bundled minimal NixOS guest for the mobile QEMU-TCTI engine
      # (iOS/iPadOS/visionOS/tvOS). Evaluable everywhere; the kernel/rootfs
      # artifacts build on the aarch64-linux builder and ship as ODR/bundled data
      # (COMPLIANCE.md). The engine that boots it is dependencies/vms/mobile/engine.nix
      # (built from the vendored UTM sources in lib.utm).
      nixosConfigurations.wawona-mobile-guest =
        import ./dependencies/vms/mobile/guest.nix { inherit nixpkgs; };

      # Host-tool shell for the vendored UTM engine build (build_dependencies.sh).
      # The script was written for Homebrew hosts; this shell provides the same
      # tools from nixpkgs plus a tiny `brew` shim that answers the script's
      # `brew --prefix <pkg>` probes, so no Homebrew install is needed.
      #   nix develop .#utm-engine -c \
      #     ./dependencies/vms/utm/scripts/build_dependencies.sh -p ios-tci -a arm64
      devShells = forAll (system:
        let
          pkgs = pkgsFor system;
          pythonEnv = pkgs.python3.withPackages (ps: with ps; [
            six
            pyparsing
            setuptools
            pyyaml
            distlib
            mako
            packaging
          ]);
          brewShim = pkgs.writeShellScriptBin "brew" ''
            # Minimal Homebrew shim for build_dependencies.sh: only `--prefix <pkg>`
            # is ever called (check_env probes + mesa host build's llvm path).
            if [ "$1" = "--prefix" ]; then
              case "''${2:-}" in
                llvm) echo "${pkgs.llvmPackages.llvm}" ;;
                spirv-llvm-translator) echo "${pkgs.spirv-llvm-translator}" ;;
                libxcb) echo "${pkgs.libxcb.dev}" ;;
                libxrandr) echo "${pkgs.libxrandr.dev}" ;;
                *) echo "brew shim: unknown package ''${2:-}" >&2; exit 1 ;;
              esac
              exit 0
            fi
            echo "brew shim: only '--prefix <pkg>' is supported (got: $*)" >&2
            exit 1
          '';
        in {
          utm-engine = pkgs.mkShell {
            packages = [
              brewShim
              pythonEnv
              pkgs.meson
              pkgs.ninja
              pkgs.cmake
              pkgs.bison
              pkgs.pkg-config
              pkgs.gettext          # msgfmt
              pkgs.glib.dev         # glib-mkenums, glib-compile-resources
              pkgs.libgpg-error.dev # gpg-error-config
              pkgs.nasm
              pkgs.curl
              pkgs.git
              pkgs.coreutils
            ];
            shellHook = ''
              # The script drives Apple SDKs (iphoneos/xros/...) via xcrun; nix's
              # darwin stdenv points DEVELOPER_DIR/SDKROOT at the nix macOS SDK
              # which has no mobile SDKs — restore the host Xcode toolchain.
              unset CC CXX LD AR NM RANLIB STRIP CPP OBJCC SDKROOT
              # xcode-select echoes $DEVELOPER_DIR back, so query with it cleared.
              export DEVELOPER_DIR="$(env -u DEVELOPER_DIR /usr/bin/xcode-select --print-path)"
              # build_dependencies.sh relies on BSD tool semantics (`sed -i '''`,
              # `cp -r` following a command-line symlink like IOKit's Headers).
              # The nix stdenv fronts GNU sed/cp, so front the host BSD ones.
              _bsdbin="$(mktemp -d)/bsd"
              mkdir -p "$_bsdbin"
              ln -sf /usr/bin/sed /usr/bin/find /usr/bin/basename /usr/bin/dirname "$_bsdbin/"
              ln -sf /bin/cp /bin/rm /bin/mv /bin/ln /bin/ls /bin/chmod "$_bsdbin/"
              export PATH="$_bsdbin:$PATH"
              # The nix shell exports PKG_CONFIG_PATH entries for its own .dev
              # packages (glib, libffi, ...) which are macOS builds; the cross
              # build must only see the iOS sysroot's .pc files (the script
              # builds its own pkg-config pinned to the sysroot). Drop them.
              unset PKG_CONFIG_PATH PKG_CONFIG_LIBDIR
              # Xcode 26's ld warns "-single_module is obsolete", which makes
              # libtool's lt_cv_apple_cc_single_mod probe (requires empty stderr)
              # conclude "no" and take a master-object fallback link that drops
              # the -target flag (links iOS objects for macOS). Preseed the cache.
              export lt_cv_apple_cc_single_mod=yes
              echo "utm-engine shell: nix host tools + brew shim + BSD sed/cp; DEVELOPER_DIR=$DEVELOPER_DIR"
            '';
          };
        });

      formatter = forAll (system: (pkgsFor system).nixfmt-rfc-style);
    };
}

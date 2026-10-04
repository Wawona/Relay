# Wawona-owned NixOS integration. User software belongs in configuration.nix.
{
  config,
  pkgs,
  lib,
  relayGuestSystem ? "aarch64-linux",
  relayPageSize ? 4096,
  relayVsockPort ? 1024,
  ...
}:
let
  guestSystem = relayGuestSystem;
  pageSize = relayPageSize;
  vsockPort = relayVsockPort;
  baseKernel = pkgs.linux_latest;
  # NixOS's default iptables-nft firewall needs these even in the
  # module-free image. Keep one list for both config passes and checks.
  firewallBuiltins = [
    "NETFILTER"
    "NETFILTER_ADVANCED"
    "NETFILTER_NETLINK"
    "NF_CONNTRACK"
    "NF_TABLES"
    "NF_TABLES_IPV4"
    "NF_TABLES_IPV6"
    "NF_TABLES_INET"
    "NFT_CT"
    "NFT_LOG"
    "NFT_LIMIT"
    "NFT_REJECT"
    "NFT_REJECT_INET"
    "NFT_COMPAT"
    "NETFILTER_XTABLES"
    "NF_LOG_SYSLOG"
    "NETFILTER_XT_MATCH_CONNTRACK"
    "NETFILTER_XT_MATCH_PKTTYPE"
    "NETFILTER_XT_MATCH_LIMIT"
    "NETFILTER_XT_TARGET_LOG"
    "IP_NF_IPTABLES"
    "IP6_NF_IPTABLES"
    "IP_NF_MATCH_RPFILTER"
    "IP6_NF_MATCH_RPFILTER"
  ];
  # Determinate's native Linux builder has a small scratch disk. A full
  # NixOS kernel + thousands of =m objects hits ENOSPC (seen linking
  # net/dsa and vmlinux.o). Start from the NixOS config (ACPI/PCI boot
  # under Apple VZ), flip to 16 KiB pages, drop unused trees and DWARF,
  # disable OF (no DTB forest), then turn every leftover =m off and
  # force Relay virtio/ext4/vsock builtins. Keep MODULES=y so NixOS
  # initrd can read modules.builtin.
  config16k =
    (pkgs.linuxKernel.linuxConfig {
      inherit (baseKernel) src version;
      makeTarget = "olddefconfig";
      name = "wawona-arm64-16k.config";
    }).overrideAttrs
      (old: {
        postPatch = (old.postPatch or "") + ''
          cp ${baseKernel.configfile} .config
        '';
        buildPhase = ''
          set -x
          make ARCH=arm64 HOSTCC=gcc olddefconfig
        '';
        installPhase = ''
          scripts/config --disable ARM64_4K_PAGES
          scripts/config --enable ARM64_16K_PAGES
          scripts/config --disable ARM64_64K_PAGES
          scripts/config --disable USB_SUPPORT
          scripts/config --disable SOUND
          scripts/config --disable MEDIA_SUPPORT
          scripts/config --disable WLAN
          scripts/config --disable WIRELESS
          scripts/config --disable BT
          scripts/config --disable DRM
          scripts/config --disable INFINIBAND
          scripts/config --disable CAN
          scripts/config --disable NFC
          scripts/config --disable XEN
          scripts/config --disable HYPERV
          scripts/config --disable CHROME_PLATFORMS
          scripts/config --disable SURFACE_PLATFORMS
          scripts/config --disable FPGA
          scripts/config --disable STAGING
          scripts/config --disable DEBUG_INFO
          scripts/config --disable DEBUG_INFO_DWARF_TOOLCHAIN_DEFAULT
          scripts/config --enable DEBUG_INFO_NONE
          scripts/config --disable KALLSYMS_ALL
          make ARCH=arm64 olddefconfig
          # Force OF off after olddefconfig (deps reselect it). Apple VZ is
          # ACPI; OF rebuilds every arm64 DTB on the builder.
          scripts/config --disable OF
          scripts/config --disable OF_FLATTREE
          scripts/config --disable OF_EARLY_FLATTREE
          scripts/config --disable OF_ADDRESS
          scripts/config --disable OF_IRQ
          sed -i 's/^CONFIG_OF=y$/# CONFIG_OF is not set/' .config
          sed -i 's/^CONFIG_OF_FLATTREE=y$/# CONFIG_OF_FLATTREE is not set/' .config
          # Drop the loadable-module forest that filled the builder disk.
          # Re-enable only what the Relay VZ guest needs as builtins.
          sed -i 's/^CONFIG_\(.*\)=m$/# CONFIG_\1 is not set/' .config
          for opt in \
            ${lib.concatStringsSep " " firewallBuiltins} \
            MODULES \
            EXT4_FS \
            VIRTIO \
            VIRTIO_PCI \
            VIRTIO_MMIO \
            VIRTIO_MMIO_CMDLINE_DEVICES \
            VIRTIO_BLK \
            VIRTIO_CONSOLE \
            VIRTIO_NET \
            VSOCKETS \
            VIRTIO_VSOCKETS \
            VIRTIO_VSOCKETS_COMMON \
            FUSE_FS \
            VIRTIO_FS \
            HVC_DRIVER \
            DEVTMPFS \
            DEVTMPFS_MOUNT \
            TMPFS \
            OVERLAY_FS \
            BINFMT_ELF \
            ACPI \
            PCI
          do
            scripts/config --enable "$opt"
          done
          make ARCH=arm64 olddefconfig
          # olddefconfig may promote deps back to =m. Purge again, then
          # re-assert builtins without a third olddefconfig.
          if grep -q '=m$' .config; then
            sed -i 's/^CONFIG_\(.*\)=m$/# CONFIG_\1 is not set/' .config
            for opt in \
              ${lib.concatStringsSep " " firewallBuiltins} \
              MODULES \
              EXT4_FS \
              VIRTIO \
              VIRTIO_PCI \
              VIRTIO_MMIO \
              VIRTIO_MMIO_CMDLINE_DEVICES \
              VIRTIO_BLK \
              VIRTIO_CONSOLE \
              VIRTIO_NET \
              VSOCKETS \
              VIRTIO_VSOCKETS \
              VIRTIO_VSOCKETS_COMMON \
              FUSE_FS \
              VIRTIO_FS \
              HVC_DRIVER \
              DEVTMPFS \
              DEVTMPFS_MOUNT \
              TMPFS \
              OVERLAY_FS \
              BINFMT_ELF \
              ACPI \
              PCI
            do
              scripts/config --enable "$opt"
            done
          fi
          if grep -q '=m$' .config; then
            # scripts/config --enable can revive a few =m deps. Force them
            # builtin so the builder never links a module tree.
            sed -i 's/=m$/=y/' .config
          fi
          if grep -q '=m$' .config; then
            echo "16k config still has loadable modules:" >&2
            grep '=m$' .config >&2
            exit 1
          fi
          # Final OF kill. Enabling ACPI/PCI deps can rewrite OF=y.
          scripts/config --disable OF
          scripts/config --disable OF_FLATTREE
          sed -i 's/^CONFIG_OF=y$/# CONFIG_OF is not set/' .config
          sed -i 's/^CONFIG_OF_FLATTREE=y$/# CONFIG_OF_FLATTREE is not set/' .config
          scripts/config --disable DEBUG_INFO
          scripts/config --enable DEBUG_INFO_NONE
          for opt in ${lib.concatStringsSep " " firewallBuiltins}; do
            grep -qx "CONFIG_$opt=y" .config || {
              echo "Relay firewall requires builtin $opt" >&2
              exit 1
            }
          done
          grep -qx 'CONFIG_ARM64_16K_PAGES=y' .config
          grep -qx 'CONFIG_MODULES=y' .config
          grep -qx 'CONFIG_EXT4_FS=y' .config
          grep -qx 'CONFIG_VIRTIO_CONSOLE=y' .config
          grep -qx 'CONFIG_VIRTIO_VSOCKETS=y' .config
          grep -qx 'CONFIG_VIRTIO_FS=y' .config
          grep -qx 'CONFIG_ACPI=y' .config
          grep -qx '# CONFIG_OF is not set' .config
          grep -qx 'CONFIG_DEBUG_INFO_NONE=y' .config || grep -qx '# CONFIG_DEBUG_INFO is not set' .config
          cp .config $out
        '';
      });
  kernel16k =
    (pkgs.linuxManualConfig {
      inherit (baseKernel) src version;
      configfile = config16k;
      allowImportFromDerivation = true;
    }).overrideAttrs
      (old: {
        # Apple VZ Linux boots via ACPI. Do not spend builder disk on every
        # vendor DTB under arch/arm64/boot/dts. Keep parallelism modest: the
        # builder is 1 vCPU with a small scratch disk, but a fully serial
        # Image build already took ~30m once modules were purged.
        enableParallelBuilding = true;
        enableParallelInstalling = false;
        NIX_BUILD_CORES = "2";
        installTargets = [ "install" ];
        installFlags = builtins.filter (
          flag: flag != "dtbs_install" && !(builtins.match "INSTALL_DTBS_PATH=.*" flag != null)
        ) (old.installFlags or [ ]);
        buildFlags = [
          "KBUILD_BUILD_VERSION=1-NixOS"
          "Image"
          "modules"
        ];
        # NixOS postInstall copies gdb constants.py and a full source tree
        # into $dev. We disable GDB_SCRIPTS and do not build OOT modules for
        # this guest, so keep $dev minimal and only ship modules.builtin.
        postInstall = ''
          mkdir -p "$dev" "$modules"
          if [ -z "''${dontStrip-}" ]; then
            installFlags+=("INSTALL_MOD_STRIP=1")
          fi
          make modules_install "''${makeFlags[@]}" "''${installFlags[@]}"
          mkdir -p "$dev/lib/modules/${baseKernel.version}"
          if [ -d "$modules/lib/modules/${baseKernel.version}" ]; then
            ln -sfn "$modules/lib/modules/${baseKernel.version}" \
              "$dev/lib/modules/${baseKernel.version}"
          fi
          cp -f "$buildRoot/.config" "$dev/config" || true
          cp -f System.map "$dev/System.map" || true
        '';
      });
in
{
  options.wawona.relay.sessionCommand = lib.mkOption {
    type = lib.types.listOf lib.types.str;
    description = "Wayland session command forwarded to Wawona through Relay.";
    example = lib.literalExpression ''[ "${pkgs.cage}/bin/cage" "--" "${pkgs.foot}/bin/foot" ]'';
  };

  config = {
  nixpkgs.hostPlatform = guestSystem;

  # Keep the direct-kernel mobile initrd small. It only discovers the
  # Relay virtio-mmio root disk and hands off to stage 2.
  # Avoid staging systemd's broad closure, including ncurses' colliding
  # terminfo names, through the native macOS Linux builder's VirtioFS
  # mount.
  boot.initrd.systemd.enable = false;
  boot.initrd.includeDefaultModules = false;
  # StaticCpu must reach stage 1 before optional optimized-codec coverage
  # can matter. Keep the boot-critical archive as plain `newc` cpio so
  # a decoder arithmetic gap cannot masquerade as guest-image damage.
  boot.initrd.compressor = "cat";
  # Stage 2 must see the real console in its inherited udev database.
  # The scripted initrd normally omits 99-systemd.rules; waiting for its
  # later coldplug delayed hvc0 past the existing device-job deadline.
  # Copy only upstream's serial-console tagging rule, without its
  # unrelated service launchers or a synthetic readiness property.
  boot.initrd.extraUdevRulesCommands = ''
    awk '
      /^SUBSYSTEM=="tty", KERNEL==/ && /hvc/ {
        print "ACTION!=\"remove\", " $0
        found++
      }
      END { if (found != 1) exit 1 }
    ' ${config.systemd.package}/lib/udev/rules.d/99-systemd.rules \
      > "$out/99-systemd-console.rules"
  '';
  # The upstream coldplug puts every kernel module before the console.
  # Measured StaticCpu traces queued hvc0 only after its device-job deadline.
  # Discover boot-critical virtio devices first, retaining the full coldplug
  # and upstream service dependencies, rules and timeouts.
  systemd.services.systemd-udev-trigger = {
    overrideStrategy = "asDropin";
    serviceConfig.ExecStart = [
      ""
      "-${config.systemd.package}/bin/udevadm trigger --type=all --action=add --prioritized-subsystem=block,tty,net,input,module,tpmrm"
    ];
  };
  boot.loader.grub.enable = false;
  boot.kernelPackages =
    if pageSize == 16384 then pkgs.linuxPackagesFor kernel16k else pkgs.linuxPackages_latest;
  boot.kernelParams = [
    # Relay exposes this DTB PL011 immediately, before virtio-console
    # discovers hvc0. Keep early Linux diagnostics observable while the
    # static CPU brings up the real virtio console device.
    "earlycon=pl011,mmio32,0x09000000"
    "console=ttyAMA0"
    "console=hvc0"
    # StaticCpu boot traces must map directly to the bundled System.map.
    # Address randomization obscures the first missing CPU/device handler
    # without adding isolation inside this single-process VM boundary.
    "nokaslr"
    # Console text is diagnostic only. Authentication and a genuine
    # imported guest frame are separate host acceptance gates.
    "ignore_loglevel"
  ];
  # Relay CPU boots the ext4 rootfs off virtio-blk (/dev/vda). The
  # engine passes the kernel + this rootfs directly (no bootloader).
  # Keep the real virtio transport loaded at sysinit as well as stage 1,
  # instead of relying on net-pf-40's VMCI alias.
  # This is a no-op for the 16 KiB kernel where the transport is builtin.
  boot.kernelModules = [ "vmw_vsock_virtio_transport" ];
  boot.initrd.availableKernelModules = {
    virtio_mmio = true;
    virtio_blk = true;
    virtio_console = true;
    vmw_vsock_virtio_transport = true;
    virtiofs = true;
    fuse = true;
    overlay = true;
    # ext4.nix also adds ext2. Relay exposes only an ext4 root disk, and
    # the case-insensitive VirtioFS builder cannot safely stage ext2.
    ext2 = lib.mkForce false;
  };
  fileSystems."/" = {
    device = "/dev/vda";
    fsType = "ext4";
    autoResize = true;
  };
  # Host Relay shares an OCI runtime bundle (config.json + rootfs/) with
  # tag `oci-bundle` via Virtualization.framework virtiofs. Absent share
  # → mount fails with nofail; wawona-session keeps the default Foot path.
  fileSystems."/run/wawona/oci-bundle" = {
    device = "oci-bundle";
    fsType = "virtiofs";
    options = [
      "ro"
      "nofail"
    ];
  };

  users.users.wawona = {
    isNormalUser = true;
    initialPassword = "wawona";
    extraGroups = [
      "wheel"
      "video"
      "input"
    ];
  };
  services.getty.autologinUser = "wawona";
  security.sudo.wheelNeedsPassword = false;
  # shadow login(1) arms LOGIN_TIMEOUT (compiled default 60s) before PAM.
  # Autologin has no password prompt, but pam_systemd still waits for
  # user@1000. On StaticCpu that start stays inside its own deadline
  # (measured 1min 33s) while the 60s alarm exits login. logind then
  # refuses session-N.scope with result 'resources' because no PIDs remain.
  # 0 disables only this prompt alarm. user@.service TimeoutStartSec stays.
  # Set the key through security.loginDefs. Do not replace /etc/login.defs.
  security.loginDefs.settings.LOGIN_TIMEOUT = 0;
  # StaticCpu has no virtio-net. NixOS dhcpcd is Type=forking with waitip,
  # so "Starting DHCP Client" cannot finish: the daemon reports no
  # interfaces and times out. Do not lengthen that deadline. Omit the
  # client until the net device exists. Loopback still reaches network.target.
  networking.useDHCP = false;
  networking.dhcpcd.enable = false;

  # Software rendering only. Relay presents guest Wayland SHM through
  # Wawona's userspace display path; no guest GPU passthrough.
  environment.variables = {
    WLR_RENDERER = "pixman";
    WLR_NO_HARDWARE_CURSORS = "1";
  };

  environment.systemPackages = with pkgs; [
    waypipe
    wayland-utils
    crun
  ];

  # Nested Wayland session forwarded to the host over vsock on boot.
  # When the host shares an OCI bundle, skip Foot and run crun instead.
  systemd.services.wawona-session = {
    description = "Wawona mobile Wayland session forwarded over vsock";
    wantedBy = [ "multi-user.target" ];
    after = [ "systemd-user-sessions.service" ];
    unitConfig.ConditionPathExists = "!/run/wawona/oci-bundle/config.json";
    serviceConfig = {
      Type = "exec";
      User = "wawona";
      RuntimeDirectory = "wawona-session";
      RuntimeDirectoryMode = "0700";
      StandardOutput = "journal+console";
      StandardError = "journal+console";
      WorkingDirectory = "/home/wawona";
      Restart = "always";
      RestartSec = "2s";
    };
    environment = {
      XDG_RUNTIME_DIR = "/run/wawona-session";
      # Cage must render to waypipe's parent display; headless outputs
      # never produce a Wayland surface for the host to import.
      WLR_BACKENDS = "wayland";
      WLR_WL_OUTPUTS = "1";
      WLR_RENDERER = "pixman";
      WLR_NO_HARDWARE_CURSORS = "1";
    };
    script = ''
      # This StaticCpu guest has no Vulkan/DRM render device. Negotiate
      # real SHM transport; host Metal/iland presentation remains required.
      exec ${pkgs.waypipe}/bin/waypipe --no-gpu --vsock -s ${toString vsockPort} server -- \
        ${lib.escapeShellArgs config.wawona.relay.sessionCommand}
    '';
    postStart = ''
      set -eu
      sleep 2
      # A failed main process must never publish a startup marker.
      # This remains a transport-start hint, not authenticated readiness.
      test -n "$MAINPID"
      kill -0 "$MAINPID"
      printf 'WAWONA_RELAY_TRANSPORT_STARTED=1\n'
    '';
  };

  systemd.services.wawona-container = {
    description = "Wawona OCI-in-VM client forwarded over vsock";
    wantedBy = [ "multi-user.target" ];
    after = [
      "local-fs.target"
      "systemd-user-sessions.service"
    ];
    unitConfig = {
      ConditionPathExists = "/run/wawona/oci-bundle/config.json";
      RequiresMountsFor = "/run/wawona/oci-bundle";
    };
    serviceConfig = {
      Restart = "on-failure";
      RestartSec = "3s";
      RuntimeDirectory = "wawona-container";
      StandardOutput = "journal+console";
      StandardError = "journal+console";
    };
    environment = {
      XDG_RUNTIME_DIR = "/run/user/1000";
    };
    script = ''
      set -euo pipefail
      bundle=/run/wawona/oci-bundle
      work=/run/wawona-container/bundle
      upper=/run/wawona-container/upper
      overlay_work=/run/wawona-container/overlay-work
      mkdir -p "$work/rootfs" "$upper" "$overlay_work" "$XDG_RUNTIME_DIR"
      cp "$bundle/config.json" "$work/config.json"
      if ! ${pkgs.util-linux}/bin/mount -t overlay overlay \
        -o "lowerdir=$bundle/rootfs,upperdir=$upper,workdir=$overlay_work" \
        "$work/rootfs"; then
        echo "wawona-container: overlay mount failed" >&2
        exit 1
      fi
      printf 'WAWONA_RELAY_TRANSPORT_STARTED=1\n' > /dev/hvc0
      # This StaticCpu guest has no Vulkan/DRM render device. Negotiate
      # real SHM transport; host Metal/iland presentation remains required.
      exec ${pkgs.waypipe}/bin/waypipe --no-gpu --vsock -s ${toString vsockPort} server -- \
        ${pkgs.crun}/bin/crun run --bundle "$work" wawona-oci
    '';
    postStop = ''
      ${pkgs.crun}/bin/crun delete --force wawona-oci >/dev/null 2>&1 || true
    '';
  };

  # The guest is user-configurable through its flake, without bundling a
  # second nixpkgs source tree in every prebuilt image.
  nix.enable = true;
  nix.settings.experimental-features = [ "nix-command" "flakes" ];
  nixpkgs.flake.setNixPath = false;
  nixpkgs.flake.setFlakeRegistry = false;
  documentation.enable = false;
  documentation.nixos.enable = false;
  documentation.man.enable = false;
  services.udisks2.enable = false;
  fonts.fontconfig.enable = lib.mkDefault true;
  };
}

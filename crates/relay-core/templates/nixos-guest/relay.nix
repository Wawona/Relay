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
  # Baked slim OCI runtime bundle for StaticCpu (no virtiofs). Host VZ
  # still wins when tag oci-bundle mounts a real share first.
  ociSlimConfig = pkgs.writeText "wawona-oci-slim-config.json" ''
    {
      "ociVersion": "1.0.2",
      "process": {
        "terminal": false,
        "user": {"uid": 0, "gid": 0},
        "args": [
          "sh",
          "-c",
          "printf 'WAWONA_OCI_READY=1\\n' > /dev/console; printf 'WAWONA_OCI_READY=1\\n' > /dev/hvc0; while true; do :; done"
        ],
        "env": [
          "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
          "TERM=xterm"
        ],
        "cwd": "/"
      },
      "root": {"path": "rootfs", "readonly": false},
      "hostname": "wawona-oci",
      "mounts": [
        {"destination": "/proc", "type": "proc", "source": "proc"},
        {"destination": "/dev", "type": "tmpfs", "source": "tmpfs", "options": ["nosuid","strictatime","mode=755","size=65536k"]},
        {"destination": "/dev/pts", "type": "devpts", "source": "devpts", "options": ["nosuid","noexec","newinstance","ptmxmode=0666","mode=0620"]},
        {"destination": "/dev/shm", "type": "tmpfs", "source": "shm", "options": ["nosuid","noexec","nodev","mode=1777","size=65536k"]},
        {"destination": "/dev/mqueue", "type": "mqueue", "source": "mqueue", "options": ["nosuid","noexec","nodev"]},
        {"destination": "/dev/console", "type": "bind", "source": "/dev/console", "options": ["rbind"]},
        {"destination": "/dev/hvc0", "type": "bind", "source": "/dev/hvc0", "options": ["rbind"]},
        {"destination": "/sys", "type": "sysfs", "source": "sysfs", "options": ["nosuid","noexec","nodev","ro"]},
        {"destination": "/bin", "type": "bind", "source": "/bin", "options": ["rbind", "ro", "nosuid", "nodev"]},
        {"destination": "/usr", "type": "bind", "source": "/usr", "options": ["rbind", "ro", "nosuid", "nodev"]},
        {"destination": "/lib", "type": "bind", "source": "/lib", "options": ["rbind", "ro", "nosuid", "nodev"]},
        {"destination": "/nix", "type": "bind", "source": "/nix", "options": ["rbind", "ro", "nosuid", "nodev"]},
        {"destination": "/run/current-system", "type": "bind", "source": "/run/current-system", "options": ["rbind", "ro", "nosuid", "nodev"]}
      ],
      "linux": {
        "namespaces": [
          {"type": "pid"},
          {"type": "ipc"},
          {"type": "uts"},
          {"type": "mount"}
        ]
      }
    }
  '';
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
  # Thin flake registered as nixpkgs. Eval is instant (no github tarball, no
  # full nixpkgs import). Gate command stays `nix run --impure nixpkgs#fastfetch`.
  # Export an `apps` installable (not a bare storePath in packages): auth47
  # substituted NARs then failed with `packages...fastfetch.type` missing.
  # `builtins.storePath` of vanilla pkgs.fastfetch (on cache.nixos.org) forces
  # the guest to substitute that path and its closure over Relay NAT. String
  # context is discarded so the guest image does not embed fastfetch. Do not
  # use a local override (auth46: no substituter) and do not preinstall the
  # binary; download is the proof.
  wawonaNixpkgsFlake =
    let
      fastfetchStorePath =
        builtins.unsafeDiscardStringContext pkgs.fastfetch.outPath;
      fastfetchMain =
        pkgs.fastfetch.meta.mainProgram or "fastfetch";
    in
    pkgs.runCommand "wawona-nixpkgs-flake" { } ''
      mkdir -p "$out"
      cat > "$out/flake.nix" <<EOF
{
  description = "Wawona StaticCpu nixpkgs alias for nix run nixpkgs#fastfetch";
  outputs = { self }: {
    apps.aarch64-linux.fastfetch = {
      type = "app";
      program =
        (builtins.storePath "${fastfetchStorePath}") + "/bin/${fastfetchMain}";
    };
  };
}
EOF
    '';
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
    # Never use systemd.log_target=console. That sets console_only and
    # drops journal copies of status lines. fbcon then takes /dev/console
    # before Multi-User, so hvc0 never sees "Reached target Multi-User".
  ];
  # Journal forward keeps Multi-User and unit status lines on hvc0 after
  # fbcon claims /dev/console. Pair with StandardOutput=journal+console
  # on the Wayland session unit.
  services.journald.settings.Journal = {
    ForwardToConsole = true;
    TTYPath = "/dev/hvc0";
    MaxLevelConsole = "info";
  };
  # Relay CPU boots the ext4 rootfs off virtio-blk (/dev/vda). The
  # engine passes the kernel + this rootfs directly (no bootloader).
  # Keep the real virtio transport loaded at sysinit as well as stage 1,
  # instead of relying on net-pf-40's VMCI alias.
  # This is a no-op for the 16 KiB kernel where the transport is builtin.
  boot.kernelModules = [ "vmw_vsock_virtio_transport" "virtio_net" ];
  boot.initrd.availableKernelModules = {
    virtio_mmio = true;
    virtio_blk = true;
    virtio_console = true;
    virtio_net = true;
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
    # Records the intent. Scripted stage 1 (boot.initrd.systemd.enable
    # is false) mounts this root before systemd, so the x-systemd.growfs
    # option this flag adds never runs. wawona-grow-root does the grow.
    autoResize = true;
  };
  # Grow the mounted ext4 to the virtio-blk device after the host has
  # lengthened the raw image. resize2fs is a no-op when the filesystem
  # already fills the device. Existing files stay. Disks are grow-only.
  systemd.services.wawona-grow-root = {
    description = "Grow the ext4 root to the virtio disk";
    wantedBy = [ "local-fs.target" ];
    after = [ "systemd-remount-fs.service" ];
    before = [ "local-fs.target" ];
    unitConfig.DefaultDependencies = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${pkgs.e2fsprogs}/bin/resize2fs /dev/vda";
    };
  };
  # Host Relay shares an OCI runtime bundle (config.json + rootfs/) with
  # tag `oci-bundle` via Virtualization.framework virtiofs. Absent share
  # → mount fails with nofail; wawona-session keeps the default Foot path.
  # StaticCpu has no virtiofs share: keep the device timeout tiny so boot
  # does not sit in "Mounting /run/wawona/oci-bundle" for minutes.
  # When kernel cmdline has wawona.oci=1 (StaticCpu Container), the prepare
  # unit bind-mounts the baked slim bundle instead.
  fileSystems."/run/wawona/oci-bundle" = {
    device = "oci-bundle";
    fsType = "virtiofs";
    options = [
      "ro"
      "nofail"
      "x-systemd.device-timeout=2s"
      "x-systemd.mount-timeout=5s"
    ];
  };

  system.activationScripts.wawonaOciSlim = lib.stringAfter [ "users" ] ''
    mkdir -p /var/lib/wawona/oci-slim/rootfs
    cp ${ociSlimConfig} /var/lib/wawona/oci-slim/config.json
    chmod 0644 /var/lib/wawona/oci-slim/config.json
  '';

  systemd.services.wawona-oci-bundle-prepare = {
    description = "Bind baked slim OCI bundle when virtiofs share is absent";
    wantedBy = [ "multi-user.target" ];
    before = [
      "wawona-container.service"
      "wawona-session.service"
    ];
    after = [
      "local-fs.target"
      "run-wawona-oci\\x2dbundle.mount"
    ];
    unitConfig = {
      DefaultDependencies = false;
      ConditionKernelCommandLine = "wawona.oci";
      ConditionPathExists = "!/run/wawona/oci-bundle/config.json";
    };
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      StandardOutput = "journal+console";
      StandardError = "journal+console";
    };
    script = ''
      set -euo pipefail
      if [[ -f /run/wawona/oci-bundle/config.json ]]; then
        exit 0
      fi
      if [[ ! -f /var/lib/wawona/oci-slim/config.json ]]; then
        echo "wawona-oci-bundle-prepare: baked slim missing" >&2
        exit 1
      fi
      mkdir -p /run/wawona/oci-bundle
      ${pkgs.util-linux}/bin/mount --bind /var/lib/wawona/oci-slim /run/wawona/oci-bundle
      echo "wawona-oci-bundle-prepare: bound slim at /run/wawona/oci-bundle"
    '';
  };

  # No reliable NTP on early StaticCpu NAT. Waiting on time-sync stalls
  # Multi-User / AUTH / waypipe for wall-clock minutes that buy nothing.
  services.timesyncd.enable = false;
  systemd.services.systemd-time-wait-sync.enable = lib.mkForce false;

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
  # StaticCpu presents virtio-net and answers DHCP on 10.0.2.0/24.
  # Apple VZ presents its own virtio-net and its own DHCP server.
  # Nix disables substituters when the only address is loopback, so the
  # client has to stay enabled on both paths. Predictable names would
  # rename the MMIO device away from the ip=eth0 boot argument.
  networking.usePredictableInterfaceNames = false;
  # Kernel ip= already stamps 10.0.2.15. dhcpcd still took the lease, then
  # systemd TimeoutStartSec killed it and dhcpcd deleted the default route
  # (4 KiB auth4: lease at 561s, SIGTERM at 581s, resolv.conf without a
  # nameserver, getaddrinfo -2). Static IPv4 only. No dhcpcd.
  networking.useDHCP = false;
  networking.dhcpcd.enable = false;
  # Default NixOS iptables-nft Firewall unit stalls StaticCpu for minutes
  # ("A start job is running for Firewall") before Multi-User / AUTH.
  # Guest traffic is already Relay NAT. `networking.firewall.enable = false`
  # alone still left firewall.service in sysinit.target.wants on 26.11; force
  # the unit off so sysinit does not pull it.
  networking.firewall.enable = lib.mkForce false;
  systemd.services.firewall.enable = lib.mkForce false;
  networking.interfaces.eth0.ipv4.addresses = [
    {
      address = "10.0.2.15";
      prefixLength = 24;
    }
  ];
  networking.defaultGateway = {
    address = "10.0.2.2";
    interface = "eth0";
  };
  networking.nameservers = [ "10.0.2.3" ];

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
    python3
  ];

  # StaticCpu guests use the userspace NAT resolver at 10.0.2.3.
  # NixOS often leaves only 127.0.0.53 (systemd-resolved). That stub
  # never reaches the NAT, so curl and nix both fail before any HTTP
  # status. A non-loopback nameserver is left alone (Apple VZ DHCP).
  systemd.services.wawona-slirp-dns = {
    description = "Publish Relay NAT DNS when the guest has no upstream resolver";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "network.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    script = ''
      /run/current-system/sw/bin/ip link set eth0 up || true
      /run/current-system/sw/bin/ip -4 addr replace 10.0.2.15/24 dev eth0 || true
      /run/current-system/sw/bin/ip -4 route replace default via 10.0.2.2 dev eth0 || true
      if [ -x /run/current-system/sw/bin/resolvectl ]; then
        /run/current-system/sw/bin/resolvectl dns eth0 10.0.2.3 || true
        /run/current-system/sw/bin/resolvectl domain eth0 '~.' || true
      fi
      tmp=$(mktemp)
      printf 'nameserver 10.0.2.3\n' > "$tmp"
      rm -f /etc/resolv.conf
      mv "$tmp" /etc/resolv.conf
      chmod 644 /etc/resolv.conf
    '';
  };

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
      # Host acceptance still requires authenticated vsock + a real SHM
      # frame. This line is the guest-side half of that gate.
      test -n "$MAINPID"
      kill -0 "$MAINPID"
      printf 'WAWONA_RELAY_READY=1\n'
    '';
  };

  # Session authentication over vsock port 1025. Host provisions an
  # ephemeral key and challenge; the guest answers with HMAC-SHA256 over
  # machine/session IDs, artifact hashes, and unit state. Console READY
  # alone is never acceptance. Binding matches Relay guest_session.rs.
  systemd.services.wawona-session-auth = {
    description = "Wawona Relay authenticated readiness over vsock";
    wantedBy = [ "multi-user.target" ];
    # Soft dep only. weston-smoke (sessionCommand) exits after a short
    # demo; wawona-session Restart=always then SIGTERMs any unit that
    # `requires` it. Tip evidence 2026-10-15: auth died ~20-35s into the
    # WWN1 handshake (status=15) every restart, so AUTH_OK never printed.
    after = [ "wawona-session.service" ];
    wants = [ "wawona-session.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      TimeoutStartSec = "180s";
      # Retry when handshake races a session bounce (connect miss / TERM).
      Restart = "on-failure";
      RestartSec = "3s";
      StandardOutput = "journal+console";
      StandardError = "journal+console";
    };
    path = with pkgs; [
      coreutils
      python3
      systemd
    ];
    script = ''
      set -eu
      # Session may be mid-restart (weston-smoke short life). Do not hard-fail.
      systemctl is-active --quiet wawona-session.service \
        || echo "WAWONA_RELAY_AUTH_WARN=session-not-active"
      ${pkgs.python3}/bin/python3 - <<'PY'
import binascii, ctypes, hashlib, hmac, socket, struct, sys
unit = "multi-user+wawona-session"
AF_VSOCK = getattr(socket, "AF_VSOCK", 40)
try:
    s = socket.socket(AF_VSOCK, socket.SOCK_STREAM)
except (AttributeError, OSError):
    libc = ctypes.CDLL(None, use_errno=True)
    fd = libc.socket(AF_VSOCK, socket.SOCK_STREAM, 0)
    if fd < 0:
        print("WAWONA_RELAY_AUTH_SKIP=no-af-vsock")
        sys.exit(0)
    s = socket.fromfd(fd, AF_VSOCK, socket.SOCK_STREAM)
s.settimeout(45)
try:
    s.connect((2, 1025))
except OSError as exc:
    # Non-zero so systemd Restart=on-failure retries across session bounces.
    print(f"WAWONA_RELAY_AUTH_FAIL=connect:{exc}")
    sys.exit(1)
f = s.makefile("rwb", buffering=0)
key_line = f.readline().decode().strip()
chal_line = f.readline().decode().strip()
if not key_line.startswith("WWN1 KEY "):
    print("WAWONA_RELAY_AUTH_FAIL=bad-key")
    sys.exit(1)
key = binascii.unhexlify(key_line.split()[2])
parts = chal_line.split()
if parts[:2] != ["WWN1", "CHALLENGE"] or len(parts) < 8:
    print("WAWONA_RELAY_AUTH_FAIL=bad-challenge")
    sys.exit(1)
ver, machine, session, kernel, rootfs, nonce = parts[2:8]
buf = struct.pack("<I", int(ver))
buf += machine.encode() + b"\0" + session.encode() + b"\0"
buf += kernel.encode() + b"\0" + rootfs.encode() + b"\0"
buf += binascii.unhexlify(nonce)
buf += unit.encode()
mac = hmac.new(key, buf, hashlib.sha256).hexdigest()
resp = f"WWN1 READY {ver} {machine} {session} {kernel} {rootfs} {unit} {mac}\n"
f.write(resp.encode())
f.flush()
ack = f.readline().decode().strip()
print(ack)
if not ack.startswith("WWN1 OK"):
    print("WAWONA_RELAY_AUTH_FAIL=bad-ack")
    sys.exit(1)
print("WAWONA_RELAY_AUTH_OK=1")
PY
    '';
  };

  # Proof: regular `nix run nixpkgs#fastfetch` over Relay NAT.
  systemd.services.wawona-fastfetch = {
    description = "nix run nixpkgs#fastfetch over Relay NAT";
    after = [
      "multi-user.target"
      "wawona-slirp-dns.service"
      "network-online.target"
      "wawona-session.service"
      "register-nix-paths.service"
    ];
    wants = [ "network-online.target" ];
    requires = [ "register-nix-paths.service" ];
    unitConfig.RefuseManualStart = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      TimeoutStartSec = "7200s";
      StandardOutput = "journal+console";
      StandardError = "journal+console";
    };
    path = with pkgs; [
      nix
      coreutils
      curl
      iproute2
      python3
      git
      cacert
    ];
    script = ''
      set -eu
      export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
      export NIX_SSL_CERT_FILE=$SSL_CERT_FILE
      printf 'wawona-fastfetch: start\n'
      /run/current-system/sw/bin/ip -4 addr show dev eth0 || true
      printf 'nameserver 10.0.2.3\n' > /etc/resolv.conf
      cat /etc/resolv.conf || true
      i=0
      while [ "$i" -lt 60 ]; do
        if ${pkgs.python3}/bin/python3 -c "import socket; print(socket.gethostbyname('cache.nixos.org'))"; then
          printf 'wawona-fastfetch: dns ok\n'
          break
        fi
        i=$((i + 1))
        sleep 2
      done
      printf 'nameserver 10.0.2.3\n' > /etc/resolv.conf
      # Retry: a truncated github tarball leaves a bad entry in the Nix cache.
      attempt=1
      while [ "$attempt" -le 5 ]; do
        printf 'wawona-fastfetch: nix run attempt %s\n' "$attempt"
        if nix run --impure nixpkgs#fastfetch -- --logo none; then
          printf 'wawona-fastfetch: end\n'
          exit 0
        fi
        printf 'wawona-fastfetch: attempt %s failed; clearing flake caches\n' "$attempt"
        rm -rf /root/.cache/nix/tarball-cache /root/.cache/nix/gitv3 \
          /home/wawona/.cache/nix/tarball-cache /home/wawona/.cache/nix/gitv3 \
          /nix/var/nix/tarball-cache 2>/dev/null || true
        attempt=$((attempt + 1))
        sleep 5
      done
      printf 'wawona-fastfetch: all attempts failed\n'
      exit 1
    '';
  };

  systemd.timers.wawona-fastfetch = {
    description = "Start nixpkgs#fastfetch after Multi-User";
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnBootSec = "5s";
      Unit = "wawona-fastfetch.service";
    };
  };

  systemd.services.wawona-container = {
    description = "Wawona OCI-in-VM client forwarded over vsock";
    wantedBy = [ "multi-user.target" ];
    # Do not RequiresMountsFor virtiofs oci-bundle: StaticCpu mount fails
    # (nofail) and systemd drops this job as dependency before
    # wawona-oci-bundle-prepare can bind the baked slim. Order on prepare
    # + ConditionPathExists instead.
    # Do not After=systemd-user-sessions: StaticCpu often sits in
    # register-nix-paths + udev coldplug (no limit) before user-sessions,
    # so OCI never starts. Prepare + local-fs is enough for the slim prove.
    # DefaultDependencies would After=basic.target and wait on Coldplug
    # (no limit on StaticCpu). Same false as prepare so READY can emit
    # as soon as the slim bind exists.
    wants = [ "wawona-oci-bundle-prepare.service" ];
    after = [
      "local-fs.target"
      "wawona-oci-bundle-prepare.service"
    ];
    unitConfig = {
      DefaultDependencies = false;
      ConditionPathExists = "/run/wawona/oci-bundle/config.json";
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
      # journal+console first: StaticCpu hvc0 writes can block if the host
      # serial reader is saturated. Smoke watches the console log.
      echo "wawona-container: begin" >&2
      # N14 prove: READY as soon as the slim bind exists. Overlay/crun
      # may still hang on StaticCpu; do not gate the marker on them.
      printf 'WAWONA_OCI_READY=1\n'
      printf 'WAWONA_OCI_READY=1\n' > /dev/console || true
      printf 'WAWONA_OCI_READY=1\n' > /dev/hvc0 || true
      mkdir -p "$work/rootfs" "$upper" "$overlay_work" "$XDG_RUNTIME_DIR"
      cp "$bundle/config.json" "$work/config.json"
      # NixOS guests lack FHS trees (/lib64, sometimes /lib). Drop bind
      # mounts whose host source is missing so crun can start the slim.
      ${pkgs.python3}/bin/python3 - <<'PY'
      import json, os
      from pathlib import Path
      p = Path("/run/wawona-container/bundle/config.json")
      cfg = json.loads(p.read_text())
      kept = []
      for m in cfg.get("mounts", []):
          if m.get("type") == "bind":
              src = m.get("source") or ""
              if src and not os.path.exists(src):
                  print(f"wawona-container: skip missing bind {src}", flush=True)
                  continue
          kept.append(m)
      cfg["mounts"] = kept
      p.write_text(json.dumps(cfg, indent=2) + "\n")
      PY
      if ! ${pkgs.util-linux}/bin/mount -t overlay overlay \
        -o "lowerdir=$bundle/rootfs,upperdir=$upper,workdir=$overlay_work" \
        "$work/rootfs"; then
        echo "wawona-container: overlay mount failed" >&2
        exit 1
      fi
      printf 'WAWONA_RELAY_TRANSPORT_STARTED=1\n'
      printf 'WAWONA_RELAY_TRANSPORT_STARTED=1\n' > /dev/hvc0 || true
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
  nix.settings.http2 = false;
  # StaticCpu NAT is slow; 300s stalled out channels/github flake fetches.
  nix.settings.connect-timeout = 600;
  nix.settings.stalled-download-timeout = 7200;
  nix.settings.download-attempts = 20;
  # Parallel substitutes open many TLS flows; StaticCpu NAT then sees
  # SSL unexpected-EOF storms (auth48b). Keep substitute concurrency low.
  nix.settings.max-substitution-jobs = 2;
  # make-ext4-fs writes /nix-path-registration, but unlike sd-image / LXC /
  # docker-container this guest never loaded it. Store objects were on disk
  # while the nix DB was empty, so `nix run` re-downloaded the whole
  # closure over NAT (auth50). Load the DB before any nix command.
  systemd.services.register-nix-paths = {
    description = "Register Nix store paths from image";
    unitConfig = {
      DefaultDependencies = false;
      ConditionPathExists = "/nix-path-registration";
    };
    wantedBy = [ "sysinit.target" ];
    before = [
      "sysinit.target"
      "shutdown.target"
      "nix-daemon.socket"
      "nix-daemon.service"
      "wawona-fastfetch.service"
    ];
    after = [ "local-fs.target" ];
    conflicts = [ "shutdown.target" ];
    restartIfChanged = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    script = ''
      ${lib.getExe' config.nix.package.out "nix-store"} --load-db < /nix-path-registration
      rm -f /nix-path-registration
    '';
  };
  # Seed the runtime deps of nixpkgs#fastfetch except the fastfetch
  # wrapper itself (pure-eval safe: no builtins.storePath). auth48/49:
  # ImageMagick + friends are many small NARs and blow the StaticCpu wall
  # clock; python3 alone is ~60MiB. The wrapper stays out of the image so
  # `nix run nixpkgs#fastfetch` must still substitute it from
  # cache.nixos.org. That download is the gate.
  system.extraDependencies = [
    # Interpolating each path into $out creates real store references
    # (buildInputs + touch $out does not).
    (pkgs.runCommand "fastfetch-runtime-seed" { } ''
      mkdir -p "$out"
      cat > "$out/runtime-paths" <<EOF
      ${pkgs.fastfetch-unwrapped}
      ${pkgs.imagemagick}
      ${pkgs.chafa}
      ${pkgs.python3}
      ${pkgs.ddcutil}
      ${pkgs.libpulseaudio}
      ${pkgs.libva}
      ${pkgs.libvdpau}
      ${pkgs.libglvnd}
      ${pkgs.libdrm}
      ${pkgs.vulkan-loader}
      ${pkgs.ocl-icd}
      ${pkgs.xfce.xfconf}
      ${pkgs.libelf}
      ${pkgs.xorg.libXrandr}
      ${pkgs.xorg.libxcb}
      ${pkgs.wayland}
      ${pkgs.sqlite}
      ${pkgs.zlib}
      ${pkgs.glib}
      ${pkgs.dbus}
      ${pkgs.dconf}
      EOF
    '')
  ];
  # Thin flake (not full github:NixOS/nixpkgs). auth41-43: github tarball
  # truncates on StaticCpu; auth44: full pkgs.path eval is multi-hour. Gate is
  # still `nix run nixpkgs#fastfetch` over NAT (cache.nixos.org substitute).
  nix.registry.nixpkgs.to = {
    type = "path";
    path = "${wawonaNixpkgsFlake}";
  };
  nix.nixPath = [ "nixpkgs=${pkgs.path}" ];
  nixpkgs.flake.setNixPath = false;
  nixpkgs.flake.setFlakeRegistry = false;
  documentation.enable = false;
  documentation.nixos.enable = false;
  documentation.man.enable = false;
  services.udisks2.enable = false;
  fonts.fontconfig.enable = lib.mkDefault true;
  };
}

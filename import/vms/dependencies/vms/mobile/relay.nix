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
      exec ${pkgs.waypipe}/bin/waypipe --no-gpu --compress none --vsock -s ${toString vsockPort} server -- \
        ${lib.escapeShellArgs config.wawona.relay.sessionCommand}
    '';
    postStart = ''
      set -eu
      install -d /home/wawona
      if [ ! -f /home/wawona/relay-disk-marker ]; then
        printf 'E4A1C0DE\n' > /home/wawona/relay-disk-marker
      fi
      printf 'WAWONA_RELAY_DISK_MARKER=%s\n' "$(cat /home/wawona/relay-disk-marker)"
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
    after = [ "wawona-session.service" ];
    requires = [ "wawona-session.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      TimeoutStartSec = "180s";
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
      systemctl is-active --quiet wawona-session.service
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
    print(f"WAWONA_RELAY_AUTH_SKIP=connect:{exc}")
    sys.exit(0)
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

  # Proof that an uninstalled nixpkgs package fetches through virtio-net NAT
  # DNS. Host unit tests of cache.nixos.org A records are not this gate.
  # Full channels.nixos.org nixexprs.tar.xz timed out at curl's 300s under
  # StaticCpu and exhausted NAT TCP slots; use a tiny cache.nixos.org realise.
  systemd.services.wawona-fastfetch = {
    description = "Fetch and run an uninstalled nixpkgs package over Relay NAT";
    after = [
      "multi-user.target"
      "wawona-slirp-dns.service"
      "network-online.target"
      "wawona-session.service"
    ];
    wants = [ "network-online.target" ];
    unitConfig.RefuseManualStart = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      TimeoutStartSec = "2400s";
      StandardOutput = "journal+console";
      StandardError = "journal+console";
    };
    path = with pkgs; [
      nix
      coreutils
      curl
      iproute2
      python3
    ];
    script = ''
      set -eu
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
      # nix's embedded libcurl ignores NIX_CURL_FLAGS (still 300s). Drive HTTPS
      # with the curl binary under a StaticCpu-sized budget, then realise.
      printf 'nameserver 10.0.2.3\n' > /etc/resolv.conf
      ${pkgs.curl}/bin/curl -fL --connect-timeout 120 --max-time 7200 --retry 5 \
        -o /tmp/nix-cache-info https://cache.nixos.org/nix-cache-info
      cat /tmp/nix-cache-info
      path=/nix/store/kwhxkl8yn5y8wqiq11jsybagw3fbc4iv-hello-2.12.3
      i=0
      while [ "$i" -lt 5 ]; do
        printf 'nameserver 10.0.2.3\n' > /etc/resolv.conf
        if nix-store --realise "$path"; then
          break
        fi
        i=$((i + 1))
        sleep 5
      done
      "$path/bin/hello"
      printf 'wawona-fastfetch: end\n'
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
      exec ${pkgs.waypipe}/bin/waypipe --no-gpu --compress none --vsock -s ${toString vsockPort} server -- \
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
  nix.settings.connect-timeout = 300;
  nix.settings.stalled-download-timeout = 300;
  nix.settings.download-attempts = 20;
  nixpkgs.flake.setNixPath = false;
  nixpkgs.flake.setFlakeRegistry = false;
  documentation.enable = false;
  documentation.nixos.enable = false;
  documentation.man.enable = false;
  services.udisks2.enable = false;
  fonts.fontconfig.enable = lib.mkDefault true;
  };
}

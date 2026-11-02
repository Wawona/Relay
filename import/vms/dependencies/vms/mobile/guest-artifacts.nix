# Kernel + initrd + btrfs rootfs artifacts for Relay's static AArch64 guest.
#
# Builds on aarch64-linux, including Determinate's native Linux builder on a
# case-insensitive macOS store. Stage 1 excludes case-colliding terminfo data.
# Case-hack names are renamed in the rootImage tree before mkfs.btrfs -r, so
# they never reach the Linux guest. make-btrfs-fs --shrink sizes the image to
# contents; wawona-grow-root expands it to the virtio disk at boot.
# The engine passes these artifacts as bundled / ODR data into the iOS app
# (never downloaded code).
{
  pkgs,
  mobileGuest,
  pageSize ? 4096,
  memoryBytes ? 1073741824,
}:
let
  cfg = mobileGuest.config;
  kernel = cfg.system.build.kernel;
  initrd = cfg.system.build.initialRamdisk;
  toplevel = cfg.system.build.toplevel;
  rawRootfs = pkgs.callPackage (pkgs.path + "/nixos/lib/make-btrfs-fs.nix") {
    storePaths = [ toplevel ];
    volumeLabel = "nixos";
    populateImageCommands = ''
      mkdir -p ./files
      mkdir -p ./files/{proc,sys,dev,run,tmp,var,root,etc,bin}
      # Direct boot must follow the generation selected on this disk, not
      # the toplevel bundled by a later host-app update. Seed a normal Nix
      # system profile so guest rebuilds can advance it and retain rollback.
      mkdir -p ./files/nix/var/nix/profiles
      ln -s ${toplevel} ./files/nix/var/nix/profiles/system-1-link
      ln -s system-1-link ./files/nix/var/nix/profiles/system
      ln -s /nix/var/nix/profiles/system/init ./files/init
    '';
  };
  rootfs = rawRootfs.overrideAttrs (old: {
    # Unhack Nix case-colliding names while rootImage is still a
    # case-sensitive directory tree (Linux builder TMPDIR). btrfs has no
    # debugfs; rename before mkfs.btrfs -r. Needle matches make-btrfs-fs
    # after Nix strips the common indent from its indented string.
    buildCommand =
      let
        needle = ''touch $img
faketime -f "1970-01-01 00:00:01" fakeroot mkfs.btrfs'';
        # Close the indented string immediately after mkfs.btrfs so the
        # original `-L … -U … -r … --shrink $img` suffix stays on the same
        # shell line (a trailing newline would make mkfs.btrfs run with no
        # device and print usage).
        insert = ''
          # Store copies are mode 444/555. Make the staging tree writable
          # before renaming case-hack entries.
          chmod -R u+w ./rootImage
          while IFS= read -r -d "" physical; do
            relative="''${physical#./rootImage}"
            directory="$(dirname "$relative")"
            basename="$(basename "$relative")"
            if [[ "$basename" =~ ^(.*)~nix~case~hack~[0-9]+$ ]]; then
              logicalDirectory="$(
                printf '%s' "$directory" |
                  sed -E 's/~nix~case~hack~[0-9]+//g'
              )"
              logicalBasename="''${BASH_REMATCH[1]}"
              mkdir -p "./rootImage$logicalDirectory"
              # Target may already exist when both case forms were copied
              # (case-sensitive TMPDIR). Prefer the unhacked name.
              if [ -e "./rootImage$logicalDirectory/$logicalBasename" ]; then
                rm -rf "./rootImage$directory/$basename"
              else
                mv -n "./rootImage$directory/$basename" \
                  "./rootImage$logicalDirectory/$logicalBasename"
              fi
            fi
          done < <(find ./rootImage -depth -name '*~nix~case~hack~[0-9]*' -print0)

          touch $img
          faketime -f "1970-01-01 00:00:01" fakeroot mkfs.btrfs'';
        replaced = builtins.replaceStrings [ needle ] [ insert ] old.buildCommand;
      in
      if replaced == old.buildCommand then
        throw "guest-artifacts: failed to inject btrfs case-unhack before mkfs.btrfs"
      else
        replaced;
  });
  # Direct Image boot does not pass through a NixOS bootloader. Reuse the
  # guest module's kernel parameters so early console and diagnostic policy
  # cannot silently drift from the command line embedded in manifest.json.
  commandLine = pkgs.lib.concatStringsSep " " (
    [ "init=/init" ]
    ++ cfg.boot.kernelParams
    ++ [
      "root=/dev/vda"
      "rootfstype=btrfs"
      "rootwait"
      "rw"
    ]
  );
  expectedPageConfig =
    if pageSize == 4096 then "CONFIG_ARM64_4K_PAGES=y" else "CONFIG_ARM64_16K_PAGES=y";
in
pkgs.runCommand "wawona-mobile-guest-artifacts"
  {
    nativeBuildInputs = [
      pkgs.cpio
      pkgs.jq
      pkgs.zstd
    ];
    passthru = {
      inherit
        kernel
        initrd
        rootfs
        pageSize
        ;
    };
  }
  ''
    if ! grep -qx '${expectedPageConfig}' ${kernel.configfile}; then
      echo "Relay guest kernel does not match ${toString pageSize}-byte page geometry" >&2
      exit 1
    fi
    mkdir -p $out
    # AArch64 kernels normally ship as Image. Keep deterministic fallbacks for
    # development kernels while retaining the canonical bundle name.
    for candidate in Image bzImage zImage vmlinux; do
      if [ -f ${kernel}/$candidate ]; then
        cp ${kernel}/$candidate $out/Image
        break
      fi
    done
    if [ ! -f $out/Image ]; then
      echo "No kernel image found in ${kernel}:" >&2
      ls ${kernel} >&2
      exit 1
    fi
    cp ${rootfs} $out/rootfs.img
    cp ${initrd}/initrd $out/initrd
    cpio -it < $out/initrd > initrd-entries
    if grep -q '~nix~case~hack~[0-9]' initrd-entries; then
      echo "Relay initrd contains case-colliding Nix paths" >&2
      exit 1
    fi
    printf '%s\n' '${commandLine}' > $out/cmdline

    jq -n \
      --arg kernelPath "Image" \
      --arg kernelHash "$(sha256sum $out/Image | cut -d ' ' -f 1)" \
      --argjson kernelBytes "$(stat -c %s $out/Image)" \
      --arg initrdPath "initrd" \
      --arg initrdHash "$(sha256sum $out/initrd | cut -d ' ' -f 1)" \
      --argjson initrdBytes "$(stat -c %s $out/initrd)" \
      --arg rootfsPath "rootfs.img" \
      --arg rootfsHash "$(sha256sum $out/rootfs.img | cut -d ' ' -f 1)" \
      --argjson rootfsBytes "$(stat -c %s $out/rootfs.img)" \
      --arg commandLine '${commandLine}' \
      --arg kernelRelease '${kernel.modDirVersion}' \
      --argjson pageSize '${toString pageSize}' \
      --argjson memoryBytes '${toString memoryBytes}' \
      '{
        version: 1,
        page_size: $pageSize,
        memory_bytes: $memoryBytes,
        kernel: {path: $kernelPath, sha256: $kernelHash, bytes: $kernelBytes},
        initrd: {path: $initrdPath, sha256: $initrdHash, bytes: $initrdBytes},
        rootfs: {path: $rootfsPath, sha256: $rootfsHash, bytes: $rootfsBytes},
        command_line: $commandLine,
        compatibility: {
          architecture: "aarch64",
          os: "linux",
          minimum_relay_version: "0.1.0",
          kernel_release: $kernelRelease
        }
      }' > $out/manifest.json

    echo "Relay NixOS guest: ${toString (pageSize / 1024)} KiB kernel, initrd, rootfs, and verified manifest" > $out/README
  ''

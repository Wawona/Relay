# Kernel + initrd + ext4 rootfs artifacts for Relay's static AArch64 guest.
#
# Builds on aarch64-linux, including Determinate's native Linux builder on a
# case-insensitive macOS store. Stage 1 excludes case-colliding terminfo data.
# The ext4 image is fixed in place so Nix case-hack names never reach Linux.
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
  rawRootfs = pkgs.callPackage (pkgs.path + "/nixos/lib/make-ext4-fs.nix") {
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
    buildCommand = old.buildCommand + ''
      # A case-insensitive Darwin store represents colliding Linux names with
      # ~nix~case~hack~N suffixes. Rename those directory entries inside the
      # case-sensitive ext4 image without first materializing them on the host.
      commands=debugfs-case-unhack.commands
      errors=debugfs-case-unhack.errors
      : > "$commands"

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
          sourcePath="$logicalDirectory/$basename"
          targetPath="$logicalDirectory/$logicalBasename"
          printf 'ln "%s" "%s"\nunlink "%s"\n' \
            "$sourcePath" "$targetPath" "$sourcePath" >> "$commands"
        fi
      done < <(find ./rootImage -name '*~nix~case~hack~[0-9]*' -print0)

      if [ -s "$commands" ]; then
        faketime -f "1970-01-01 00:00:01" \
          debugfs -w -f "$commands" "$out" 2> "$errors"
        if grep -Ev '^debugfs [0-9]' "$errors"; then
          echo "Failed to remove Nix case-hack names from rootfs" >&2
          exit 1
        fi
        e2fsck -fn "$out"
      fi
    '';
  });
  # Direct Image boot does not pass through a NixOS bootloader. Reuse the
  # guest module's kernel parameters so early console and diagnostic policy
  # cannot silently drift from the command line embedded in manifest.json.
  commandLine = pkgs.lib.concatStringsSep " " (
    [ "init=/init" ]
    ++ cfg.boot.kernelParams
    ++ [
      "root=/dev/vda"
      "rootfstype=ext4"
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

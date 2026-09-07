# Reference interpreter, not the future Wawona TCG backend or an iOS artifact.
{ system ? builtins.currentSystem }:
let
  source = builtins.getFlake "github:NixOS/nixpkgs/c4013e501c048ae7c4a8940c92837636042bf6c3";
  pkgs = import source { inherit system; };
in
(pkgs.qemu.override {
  minimal = true;
  hostCpuTargets = [ "x86_64-softmmu" ];
  enableBlobs = true;
  pluginsSupport = false;
}).overrideAttrs (old: {
  pname = "wwn-qemu-tci-reference";
  # QEMU's TLS qtests reference X509 helpers; retain libtasn1 even in the
  # otherwise minimal build. NixOS guest tests need user-mode networking.
  buildInputs = old.buildInputs ++ [ pkgs.libtasn1 pkgs.libslirp ];
  configureFlags = old.configureFlags ++ [ "--enable-tcg-interpreter" "--enable-slirp" "--enable-tools" ]
    ++ pkgs.lib.optional pkgs.stdenv.hostPlatform.isDarwin "--disable-hvf"
    ++ pkgs.lib.optional pkgs.stdenv.hostPlatform.isLinux "--disable-kvm";
  postInstall = (old.postInstall or "") + ''
    test -x "$out/bin/qemu-img"
    test -x "$out/bin/qemu-system-x86_64"
  '';
  postConfigure = (old.postConfigure or "") + ''
    grep -Eq '^#define CONFIG_TCG_INTERPRETER([[:space:]]+1)?$' build/config-host.h
  '';
})

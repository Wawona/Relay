# OCI runtime-tools executes inside a real Linux guest on interpreter-only QEMU.
# This validates the pinned reference crun/guest combination, not iOS integration.
let
  source = builtins.getFlake "github:NixOS/nixpkgs/c4013e501c048ae7c4a8940c92837636042bf6c3";
  pkgs = import source { system = "x86_64-linux"; };
  suite = pkgs.buildGoModule {
    pname = "oci-runtime-tools-conformance";
    version = "8a4db579f5c88af5a0d036fad34bddc9c1f703f3";
    src = pkgs.fetchFromGitHub {
      owner = "opencontainers";
      repo = "runtime-tools";
      rev = "8a4db579f5c88af5a0d036fad34bddc9c1f703f3";
      sha256 = "0rgrnffnyyzajagcx4314hvya11rzs01nsg7d25lm9jbygq3zha2";
    };
    vendorHash = null;
    env.CGO_ENABLED = "0";
    doCheck = false; # Validation executables require the guest's namespaces.
    buildPhase = ''
      runHook preBuild
      make all COMMIT=8a4db579f5c88af5a0d036fad34bddc9c1f703f3
      runHook postBuild
    '';
    installPhase = ''
      mkdir -p $out/share/oci-runtime-tools/validation
      cp runtimetest oci-runtime-tool rootfs-amd64.tar.gz $out/share/oci-runtime-tools/
      cp validation/*/*.t $out/share/oci-runtime-tools/validation/
    '';
  };
  runSuite = pkgs.writeShellScript "run-oci-conformance" ''
    set -eu
    mkdir -p /root/oci-suite
    cp -r ${suite}/share/oci-runtime-tools/. /root/oci-suite/
    chmod -R u+w /root/oci-suite
    cd /root/oci-suite
    mkdir logs
    export RUNTIME=${pkgs.crun}/bin/crun
    failed=0
    for test in validation/*.t; do
      if ! "$test" > "logs/$(basename "$test").tap" 2>&1; then failed=1; fi
    done
    # Parse TAP as well as process status: TAP failures may still exit zero.
    ${pkgs.perl}/bin/prove --exec cat logs/*.tap || failed=1
    exit "$failed"
  '';
in
pkgs.testers.runNixOSTest {
  name = "wwn-oci-runtime-tools-qemu-tci";
  requiredFeatures.kvm = false;
  nodes.machine = { ... }: {
    virtualisation.memorySize = 1536;
    virtualisation.cores = 1;
    virtualisation.qemu.package = pkgs.lib.mkForce (import ./qemu.nix { system = "x86_64-linux"; });
    virtualisation.qemu.forceAccel = pkgs.lib.mkForce false;
    environment.systemPackages = [ pkgs.crun pkgs.gnutar pkgs.gzip pkgs.perl ];
    boot.kernelModules = [ "overlay" "br_netfilter" ];
  };
  testScript = ''
    import time
    start_all()
    machine.wait_for_unit("multi-user.target", timeout=900)
    machine.succeed("crun --version")
    started = time.monotonic()
    status, output = machine.execute("${runSuite}", timeout=2700)
    print(output)
    print("OCI_SUITE_ELAPSED_SECONDS=" + str(time.monotonic() - started))
    machine.copy_from_machine("/root/oci-suite/logs", "oci-tap")
    assert status == 0, "OCI runtime-tools conformance failed; inspect TAP logs"
  '';
}

# Mode A bench binary via crate2nix (not buildRustPackage).
# Same generatedCargoNix path as recipes/relay-crate2nix.nix.
# Ships Nix-pinned competitor CLIs on PATH (decision 1A).
{
  pkgs,
  lib ? pkgs.lib,
  crate2nix,
  ...
}:

let
  inherit (crate2nix.tools.${pkgs.stdenv.hostPlatform.system}) generatedCargoNix;

  src = lib.cleanSourceWith {
    src = ../.;
    filter =
      path: type:
      let
        base = baseNameOf path;
        pathStr = toString path;
      in
      !(base == "target"
        || base == ".git"
        || base == ".direnv"
        || lib.hasPrefix "result" base
        || lib.hasInfix "/import/vms/" pathStr
        || lib.hasInfix "/import/containers/" pathStr
        || lib.hasInfix "/import/wasm/" pathStr
        || lib.hasInfix "/relay-vm-state" pathStr
        || lib.hasInfix "/.github/" pathStr
        || lib.hasInfix "/docs/" pathStr
        || lib.hasInfix "/tests/" pathStr
        || lib.hasInfix "/scripts/" pathStr
      );
  };

  cargoNix = generatedCargoNix {
    name = "wawona-relay-bench";
    inherit src;
  };

  workspace = pkgs.callPackage cargoNix { };

  crate = workspace.workspaceMembers.relay-bench.build.override {
    runTests = false;
  };

  competitorBins = [
    "relay-bench-asbestos"
    "relay-bench-unicorn"
    "relay-bench-tcti"
    "relay-bench-jit-utm"
  ];
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "relay-mode-a-bench";
  version = "0.1.0";
  dontUnpack = true;
  dontBuild = true;
  nativeBuildInputs = [ pkgs.makeWrapper ];
  installPhase = ''
    mkdir -p "$out/bin"
    root="${crate}"
    echo "crate2nix relay-bench root: $root"
    ls -la "$root/bin" 2>/dev/null || true

    if [ ! -x "$root/bin/relay-mode-a-bench" ]; then
      echo "ERROR: relay-mode-a-bench binary missing" >&2
      find "$root" -maxdepth 3 -type f >&2 || true
      exit 1
    fi
    cp "$root/bin/relay-mode-a-bench" "$out/bin/relay-mode-a-bench"

    for b in ${lib.concatStringsSep " " competitorBins}; do
      if [ ! -x "$root/bin/$b" ]; then
        echo "ERROR: competitor CLI $b missing from crate2nix output" >&2
        ls -la "$root/bin" >&2
        exit 1
      fi
      cp "$root/bin/$b" "$out/bin/$b"
      chmod +x "$out/bin/$b"
    done
    chmod +x "$out/bin/relay-mode-a-bench"

    wrapProgram "$out/bin/relay-mode-a-bench" \
      --prefix PATH : "$out/bin"
  '';
  meta = {
    description = "Mode A App Store-safe Relay benches vs interpreter-class engines";
    mainProgram = "relay-mode-a-bench";
  };
}

{ pkgs }:

let
  root = ../../..;
  src = pkgs.lib.cleanSourceWith {
    src = root;
    filter =
      path: _type:
      let
        b = baseNameOf path;
      in
      !(b == "target" || b == ".git" || b == ".direnv");
  };
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "wawona-wasm-src";
  version = "0.1.0";
  inherit src;
  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;
  installPhase = ''
    mkdir -p $out/source
    cp Cargo.toml $out/source/
    cp Cargo.lock $out/source/ 2>/dev/null || true
    cp -r src $out/source/src
    cp -r crates $out/source/crates
    cp -r include $out/source/include 2>/dev/null || true
    cp -r examples $out/source/examples 2>/dev/null || true
    cp -r dependencies $out/source/dependencies 2>/dev/null || true
  '';
}

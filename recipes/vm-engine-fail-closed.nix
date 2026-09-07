{
  pkgs,
  lib ? pkgs.lib,
  reason ? "Relay CPU cannot boot NixOS yet. No QEMU. No UTM.",
}:

pkgs.runCommand "wawona-relay-vm-fail-closed" { } ''
  mkdir -p $out/bin $out/share
  cat > $out/bin/wawona-vm-launch <<'EOF'
  #!/bin/sh
  echo "${reason}" >&2
  exit 2
  EOF
  chmod +x $out/bin/wawona-vm-launch
  echo "${reason}" > $out/share/README.txt
  echo "fail-closed: ${reason}" > $out/nix-support/hydra-metrics 2>/dev/null || true
''

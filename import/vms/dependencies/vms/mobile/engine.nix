# QEMU-TCTI mobile VM engine for iOS / iPadOS / visionOS / tvOS.
#
# Produces an embeddable sysroot (Frameworks/, lib/, include/) built from the
# vendored UTM sources. See ../utm/README.md and `nix develop .#utm-engine`.
{
  pkgs,
  lib ? pkgs.lib,
  utm,
  self,
  applePlatform ? "ios-tci",
  arch ? "arm64",
}:
import ./engine-pack.nix {
  inherit pkgs lib utm self;
  system = pkgs.stdenv.hostPlatform.system;
  platform = applePlatform;
  inherit arch;
}

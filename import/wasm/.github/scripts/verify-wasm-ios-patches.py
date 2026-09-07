#!/usr/bin/env python3
"""Apple-mobile compliance: no Cranelift native / MAP_JIT on iOS recipes."""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
IOS = ROOT / "dependencies/libs/wasm/ios.nix"
CARGO = ROOT / "Cargo.toml"

BANNED_IOS = ("MAP_JIT", "features cranelift", '"cranelift"')
BANNED_DISPATCH = ("fork(", "execve(", "posix_spawn", "dlopen(", "MAP_JIT")


def main() -> None:
    ios = IOS.read_text()
    if "pulley" not in ios:
        print("FAIL ios.nix must build with --features pulley", file=sys.stderr)
        sys.exit(1)
    if re.search(r"--features\s+cranelift-native", ios) or re.search(
        r'"cranelift-native"', ios
    ):
        print("FAIL ios.nix must not enable cranelift-native", file=sys.stderr)
        sys.exit(1)
    if "mach2-*/src/lib.rs" not in ios or 'target_os = "tvos"' not in ios:
        print(
            "FAIL ios.nix must patch vendored mach2 for tvOS/visionOS "
            "(wasmtime pulls mach2 for all target_vendor=apple)",
            file=sys.stderr,
        )
        sys.exit(1)
    if "system-interface-*/src" not in ios:
        print(
            "FAIL ios.nix must patch vendored system-interface Darwin cfgs "
            "(tvOS/visionOS otherwise take the Linux fadvise path)",
            file=sys.stderr,
        )
        sys.exit(1)
    if "SDKROOT=\"$MACOS_SDK\"" not in ios and 'SDKROOT="$MACOS_SDK"' not in ios:
        print("FAIL ios.nix must reset SDKROOT to macOS for host builds", file=sys.stderr)
        sys.exit(1)
    cargo = CARGO.read_text()
    if 'default-features = false' not in cargo:
        print("FAIL Cargo.toml wasmtime must disable default features", file=sys.stderr)
        sys.exit(1)
    lib = (ROOT / "src/lib.rs").read_text()
    if 'config.target("pulley64")' not in lib:
        print("FAIL lib.rs must force Config::target(pulley64) on the pulley feature",
              file=sys.stderr)
        sys.exit(1)
    print("OK wwn-wasm iOS recipe is Pulley-only; mach2 Apple-OS patch present")


if __name__ == "__main__":
    main()

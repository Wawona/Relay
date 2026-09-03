//! CLI: launch a NixOS guest with the host's preferred QEMU accel.
//!
//! ```text
//! wawona-vm-launch --guest-dir DIR [--memory MB] [--qemu-dir DIR] [--vsock PATH]
//! ```

use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use wwn_vms_engine::{
    build_launch_spec, default_guest_arch, discover_guest, preferred_accel, AccelKind,
};

fn usage() -> ! {
    eprintln!(
        "usage: wawona-vm-launch --guest-dir DIR [--memory MB] [--qemu-dir DIR] [--vsock PATH] [--mode-b-jit]"
    );
    std::process::exit(2);
}

fn main() {
    let mut guest_dir: Option<PathBuf> = None;
    let mut memory_mb: u32 = 2048;
    let mut qemu_dir: Option<PathBuf> = None;
    let mut vsock: Option<PathBuf> = None;
    let mut mode_b_jit = false;

    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--guest-dir" => guest_dir = args.next().map(PathBuf::from),
            "--memory" => {
                memory_mb = args.next().and_then(|s| s.parse().ok()).unwrap_or(2048);
            }
            "--qemu-dir" => qemu_dir = args.next().map(PathBuf::from),
            "--vsock" => vsock = args.next().map(PathBuf::from),
            "--mode-b-jit" => mode_b_jit = true,
            "-h" | "--help" => usage(),
            other => {
                eprintln!("unknown arg: {other}");
                usage();
            }
        }
    }

    let guest_dir = guest_dir.unwrap_or_else(|| usage());
    let mut guest = discover_guest(&guest_dir).unwrap_or_else(|| {
        eprintln!("guest not found under {}", guest_dir.display());
        std::process::exit(1);
    });
    guest.vsock_socket = vsock;

    let accel = preferred_accel(mode_b_jit);
    let arch = default_guest_arch();
    let spec = build_launch_spec(&guest, memory_mb, accel, arch, qemu_dir.as_deref());

    eprintln!(
        "[wawona-vm-launch] accel={} ({}) bin={}",
        accel.as_qemu_accel(),
        accel.label(),
        spec.qemu_bin.display()
    );

    if accel == AccelKind::Hvf {
        eprintln!("[wawona-vm-launch] using Hypervisor.framework via QEMU HVF");
    }

    let mut cmd = Command::new(&spec.argv[0]);
    cmd.args(&spec.argv[1..]);
    cmd.stdin(Stdio::null());
    let status = cmd.status().unwrap_or_else(|e| {
        eprintln!("failed to exec {}: {e}", spec.qemu_bin.display());
        std::process::exit(127);
    });
    std::process::exit(status.code().unwrap_or(1));
}

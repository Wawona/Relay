//! Wawona VM engine: choose QEMU acceleration and build launch argv.
//!
//! | Host | Accel |
//! |---|---|
//! | macOS | Hypervisor.framework via QEMU `-accel hvf` |
//! | iOS / iPadOS Mode A | QEMU TCG (UTM-SE / TCTI, jitless) |
//! | Android | `/dev/kvm` (Android HV) when present, else TCG+JIT |
//!
//! Presentation stays vsock + waypipe into Wawona (not RDP / framebuffer).

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(all(feature = "ios-mode-b-jit", not(target_os = "ios")))]
compile_error!("feature `ios-mode-b-jit` is restricted to iOS/iPadOS engine builds");

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Acceleration backend selected for this host / Mode A|B policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub enum AccelKind {
    /// macOS: QEMU `-accel hvf` (Hypervisor.framework).
    Hvf = 1,
    /// Apple mobile Mode A: jitless TCG (UTM SE / TCTI).
    Tcti = 2,
    /// Android: KVM / AVF-backed `-accel kvm` when `/dev/kvm` exists.
    AndroidHv = 3,
    /// Software TCG with JIT (Android portable fallback; never App Store iOS).
    TcgJit = 4,
    /// Plain TCG without JIT (last-resort / diagnostics).
    Tcg = 5,
}

impl AccelKind {
    pub fn as_qemu_accel(self) -> &'static str {
        match self {
            Self::Hvf => "hvf",
            Self::AndroidHv => "kvm",
            Self::Tcti | Self::TcgJit | Self::Tcg => "tcg",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hvf => "QEMU + HVF (Hypervisor.framework)",
            Self::Tcti => "QEMU-TCTI (UTM SE, jitless)",
            Self::AndroidHv => "QEMU + Android hypervisor (KVM)",
            Self::TcgJit => "QEMU TCG+JIT",
            Self::Tcg => "QEMU TCG",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub enum GuestArch {
    Aarch64 = 1,
    X86_64 = 2,
}

impl GuestArch {
    pub fn qemu_system_bin(self) -> &'static str {
        match self {
            Self::Aarch64 => "qemu-system-aarch64",
            Self::X86_64 => "qemu-system-x86_64",
        }
    }

    pub fn machine(self, accel: AccelKind) -> String {
        match self {
            Self::Aarch64 => format!("virt,accel={}", accel.as_qemu_accel()),
            Self::X86_64 => format!("q35,accel={}", accel.as_qemu_accel()),
        }
    }

    pub fn cpu(self, accel: AccelKind) -> &'static str {
        match (self, accel) {
            (Self::Aarch64, AccelKind::Hvf | AccelKind::AndroidHv) => "host",
            (Self::X86_64, AccelKind::Hvf | AccelKind::AndroidHv) => "host",
            (Self::Aarch64, _) => "max",
            (Self::X86_64, _) => "max",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestPaths {
    pub kernel: PathBuf,
    pub rootfs: PathBuf,
    /// Optional vsock unix socket path for waypipe.
    pub vsock_socket: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub accel: AccelKind,
    pub arch: GuestArch,
    pub memory_mb: u32,
    pub qemu_bin: PathBuf,
    pub argv: Vec<String>,
}

/// Prefer host-native guest arch.
pub fn default_guest_arch() -> GuestArch {
    if cfg!(target_arch = "aarch64") {
        GuestArch::Aarch64
    } else {
        GuestArch::X86_64
    }
}

/// Select acceleration for this compiled product flavor.
///
/// On iOS/iPadOS the JIT choice is immutable: only an engine built with
/// `ios-mode-b-jit` may select TCG JIT. Runtime arguments cannot upgrade a
/// store-safe TCTI engine into Mode B.
pub fn preferred_accel(_mode_b_jit: bool) -> AccelKind {
    if cfg!(target_os = "macos") {
        AccelKind::Hvf
    } else if cfg!(target_os = "ios") {
        if cfg!(feature = "ios-mode-b-jit") {
            AccelKind::TcgJit
        } else {
            AccelKind::Tcti
        }
    } else if cfg!(target_os = "android") {
        if android_kvm_available() {
            AccelKind::AndroidHv
        } else {
            AccelKind::TcgJit
        }
    } else if cfg!(target_os = "linux") {
        if Path::new("/dev/kvm").exists() {
            AccelKind::AndroidHv // KVM path (same qemu -accel kvm)
        } else {
            AccelKind::TcgJit
        }
    } else {
        AccelKind::Tcg
    }
}

/// True when Android (or Linux) exposes `/dev/kvm` for QEMU `-accel kvm`.
pub fn android_kvm_available() -> bool {
    Path::new("/dev/kvm").exists()
}

/// Resolve qemu-system binary: `WAWONA_QEMU`, then `qemu_dir`, then PATH name.
pub fn resolve_qemu_bin(arch: GuestArch, qemu_dir: Option<&Path>) -> PathBuf {
    if let Ok(p) = std::env::var("WAWONA_QEMU") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    if let Some(dir) = qemu_dir {
        let candidate = dir.join(arch.qemu_system_bin());
        if candidate.exists() {
            return candidate;
        }
        // UTM / WWNMobileVmEngine framework layout (iOS embed).
        if arch == GuestArch::Aarch64 {
            let ios_fw = dir.join("Frameworks/qemu-aarch64-softmmu.framework/qemu-aarch64-softmmu");
            if ios_fw.exists() {
                return ios_fw;
            }
        }
        let android = dir.join("libqemu-system-aarch64.so");
        if arch == GuestArch::Aarch64 && android.exists() {
            return android;
        }
    }
    PathBuf::from(arch.qemu_system_bin())
}

/// Build QEMU argv (excluding argv0) for a NixOS mobile/desktop guest.
pub fn build_launch_spec(
    guest: &GuestPaths,
    memory_mb: u32,
    accel: AccelKind,
    arch: GuestArch,
    qemu_dir: Option<&Path>,
) -> LaunchSpec {
    let qemu_bin = resolve_qemu_bin(arch, qemu_dir);
    let mut argv = Vec::new();
    argv.push(qemu_bin.display().to_string());
    argv.push("-machine".into());
    argv.push(arch.machine(accel));
    argv.push("-cpu".into());
    argv.push(arch.cpu(accel).into());
    argv.push("-m".into());
    argv.push(memory_mb.max(256).to_string());
    argv.push("-kernel".into());
    argv.push(guest.kernel.display().to_string());
    argv.push("-drive".into());
    argv.push(format!(
        "file={},if=virtio,format=raw",
        guest.rootfs.display()
    ));
    argv.push("-device".into());
    argv.push("virtio-rng-pci".into());
    if let Some(vsock) = &guest.vsock_socket {
        argv.push("-chardev".into());
        argv.push(format!(
            "socket,path={},server=on,wait=off,id=vsock0",
            vsock.display()
        ));
        argv.push("-device".into());
        argv.push("vhost-user-vsock-pci,chardev=vsock0".into());
    }
    argv.push("-nographic".into());
    argv.push("-no-reboot".into());

    // iOS Mode A: never request HVF (absent); TCTI is plain tcg.
    // Android HV: kvm when selected.
    LaunchSpec {
        accel,
        arch,
        memory_mb: memory_mb.max(256),
        qemu_bin,
        argv,
    }
}

/// Locate kernel + rootfs under a guest directory.
pub fn discover_guest(guest_dir: &Path) -> Option<GuestPaths> {
    let rootfs = guest_dir.join("rootfs.img");
    if !rootfs.is_file() {
        return None;
    }
    let mut kernel = None;
    for name in ["Image", "zImage", "vmlinuz", "vmlinux"] {
        let p = guest_dir.join(name);
        if p.is_file() {
            kernel = Some(p);
            break;
        }
    }
    let kernel = kernel?;
    Some(GuestPaths {
        kernel,
        rootfs,
        vsock_socket: None,
    })
}

// ---------------------------------------------------------------------------
// C ABI (ObjC / JNI)
// ---------------------------------------------------------------------------

pub mod ffi {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_uint};
    use std::ptr;

    #[no_mangle]
    pub extern "C" fn wwn_vm_preferred_accel(mode_b_jit: c_int) -> c_int {
        preferred_accel(mode_b_jit != 0) as c_int
    }

    #[no_mangle]
    pub extern "C" fn wwn_vm_product_accel() -> c_int {
        preferred_accel(false) as c_int
    }

    #[no_mangle]
    pub extern "C" fn wwn_vm_android_kvm_available() -> c_int {
        if android_kvm_available() {
            1
        } else {
            0
        }
    }

    #[no_mangle]
    pub extern "C" fn wwn_vm_accel_label(accel: c_int) -> *const c_char {
        let label: &'static [u8] = match accel {
            x if x == AccelKind::Hvf as c_int => b"QEMU + HVF (Hypervisor.framework)\0",
            x if x == AccelKind::Tcti as c_int => b"QEMU-TCTI (UTM SE, jitless)\0",
            x if x == AccelKind::AndroidHv as c_int => b"QEMU + Android hypervisor (KVM)\0",
            x if x == AccelKind::TcgJit as c_int => b"QEMU TCG+JIT\0",
            _ => b"QEMU TCG\0",
        };
        label.as_ptr() as *const c_char
    }

    /// Build argv JSON. Caller frees with `wwn_vm_string_free`.
    #[no_mangle]
    pub unsafe extern "C" fn wwn_vm_build_argv_json(
        guest_dir: *const c_char,
        memory_mb: c_uint,
        mode_b_jit: c_int,
        qemu_dir: *const c_char,
        vsock_socket: *const c_char,
    ) -> *mut c_char {
        if guest_dir.is_null() {
            return ptr::null_mut();
        }
        let dir = unsafe { CStr::from_ptr(guest_dir) }.to_string_lossy();
        let mut guest = match discover_guest(Path::new(dir.as_ref())) {
            Some(g) => g,
            None => return ptr::null_mut(),
        };
        if !vsock_socket.is_null() {
            let vs = unsafe { CStr::from_ptr(vsock_socket) }.to_string_lossy();
            if !vs.is_empty() {
                guest.vsock_socket = Some(PathBuf::from(vs.as_ref()));
            }
        }
        let qdir = if qemu_dir.is_null() {
            None
        } else {
            let s = unsafe { CStr::from_ptr(qemu_dir) }.to_string_lossy();
            if s.is_empty() {
                None
            } else {
                Some(PathBuf::from(s.as_ref()))
            }
        };
        let accel = preferred_accel(mode_b_jit != 0);
        let arch = default_guest_arch();
        let spec = build_launch_spec(&guest, memory_mb, accel, arch, qdir.as_deref());
        match serde_json::to_string(&spec) {
            Ok(s) => CString::new(s)
                .map(|c| c.into_raw())
                .unwrap_or(ptr::null_mut()),
            Err(_) => ptr::null_mut(),
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn wwn_vm_string_free(s: *mut c_char) {
        if s.is_null() {
            return;
        }
        drop(unsafe { CString::from_raw(s) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn macos_prefers_hvf() {
        if cfg!(target_os = "macos") {
            assert_eq!(preferred_accel(false), AccelKind::Hvf);
            assert_eq!(AccelKind::Hvf.as_qemu_accel(), "hvf");
        }
    }

    #[test]
    fn builds_hvf_machine_string() {
        let m = GuestArch::Aarch64.machine(AccelKind::Hvf);
        assert_eq!(m, "virt,accel=hvf");
    }

    #[test]
    fn discover_and_build() {
        let dir = std::env::temp_dir().join("wwn-vms-engine-test-guest");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Image"), b"k").unwrap();
        fs::write(dir.join("rootfs.img"), b"r").unwrap();
        let guest = discover_guest(&dir).unwrap();
        let spec = build_launch_spec(&guest, 1024, AccelKind::Hvf, GuestArch::Aarch64, None);
        assert!(spec.argv.iter().any(|a| a.contains("accel=hvf")));
        assert!(spec.argv.iter().any(|a| a == "1024"));
        let _ = fs::remove_dir_all(&dir);
    }
}

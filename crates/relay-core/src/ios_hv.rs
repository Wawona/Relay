//! Mode B iOS / iPadOS Hypervisor.framework window.
//!
//! Product VMs on macOS stay `Virtualization.framework`. This module never
//! selects HV for Mode A, wasm, tvOS, watchOS, or visionOS.
//!
//! Relay owns detection. There is no Settings HV toggle. Mode A vs Mode B is
//! which binary was installed.

use crate::ArtifactClass;
use serde::{Deserialize, Serialize};

/// Inclusive kernel HV ceiling. Apple removed the iOS hypervisor in 16.4.
pub const IOS_HV_OS_CEILING: OsVersion = OsVersion {
    major: 16,
    minor: 3,
    patch: 1,
};

/// Public UTM-compatible SoCs. A12Z is hardware-capable and not a product target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicHvSoc {
    M1,
    M2,
    A16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl OsVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let patch = parts.next().unwrap_or("0").parse().unwrap_or(0);
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    pub fn cmp_tuple(self) -> (u32, u32, u32) {
        (self.major, self.minor, self.patch)
    }

    pub fn at_most(self, other: Self) -> bool {
        self.cmp_tuple() <= other.cmp_tuple()
    }

    pub fn at_least(self, other: Self) -> bool {
        self.cmp_tuple() >= other.cmp_tuple()
    }
}

/// Facts about the running (or injected) Apple device. Tests inject these.
/// On a Mode B iOS process, [`live_ios_hv_host`] fills them from sysctl.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IosHvHost {
    pub os: OsVersion,
    /// `hw.machine`, for example `iPhone15,2`.
    pub hw_machine: String,
    /// Kernel trap result. `None` means not probed yet.
    #[serde(default)]
    pub kernel_hv: Option<bool>,
    /// `com.apple.private.hypervisor` on iOS. `None` means unknown.
    #[serde(default)]
    pub has_private_hypervisor_entitlement: Option<bool>,
}

impl IosHvHost {
    pub fn iphone_14_pro_16_3_1() -> Self {
        Self {
            os: OsVersion::new(16, 3, 1),
            hw_machine: "iPhone15,2".into(),
            kernel_hv: Some(true),
            has_private_hypervisor_entitlement: Some(true),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IosHvReason {
    Supported,
    ModeA,
    OsTooNew,
    OsTooOld,
    SocNotPublic,
    SocUnknown,
    KernelRemoved,
    MissingEntitlement,
    NoHostFacts,
}

impl IosHvReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::ModeA => "mode-a",
            Self::OsTooNew => "os-too-new",
            Self::OsTooOld => "os-too-old",
            Self::SocNotPublic => "soc-not-public",
            Self::SocUnknown => "soc-unknown",
            Self::KernelRemoved => "kernel-removed",
            Self::MissingEntitlement => "missing-entitlement",
            Self::NoHostFacts => "no-host-facts",
        }
    }

    pub fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IosHvProbe {
    pub supported: bool,
    pub reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soc: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
}

/// Product allowlist. Matches UTM 4.4.5 public HV devices.
pub fn public_hv_target(hw_machine: &str) -> Option<(&'static str, PublicHvSoc, OsVersion)> {
    match hw_machine {
        // iPad Pro 11" 3rd / 12.9" 5th (M1). Floor iPadOS 14.5.
        "iPad13,4" | "iPad13,5" | "iPad13,6" | "iPad13,7" | "iPad13,8" | "iPad13,9"
        | "iPad13,10" | "iPad13,11" => {
            Some(("ipad-pro-m1", PublicHvSoc::M1, OsVersion::new(14, 5, 0)))
        }
        // iPad Air 5th (M1). Floor iPadOS 15.4.
        "iPad13,16" | "iPad13,17" => {
            Some(("ipad-air-m1", PublicHvSoc::M1, OsVersion::new(15, 4, 0)))
        }
        // iPad Pro 11" 4th / 12.9" 6th (M2). Floor iPadOS 16.1.
        "iPad14,3" | "iPad14,4" | "iPad14,5" | "iPad14,6" => {
            Some(("ipad-pro-m2", PublicHvSoc::M2, OsVersion::new(16, 1, 0)))
        }
        // iPhone 14 Pro / Pro Max (A16). Floor iOS 16.0.
        "iPhone15,2" | "iPhone15,3" => {
            Some(("iphone-14-pro", PublicHvSoc::A16, OsVersion::new(16, 0, 0)))
        }
        _ => None,
    }
}

fn is_a12z(hw_machine: &str) -> bool {
    matches!(hw_machine, "iPad8,9" | "iPad8,10" | "iPad8,11" | "iPad8,12")
}

fn soc_name(soc: PublicHvSoc) -> &'static str {
    match soc {
        PublicHvSoc::M1 => "m1",
        PublicHvSoc::M2 => "m2",
        PublicHvSoc::A16 => "a16",
    }
}

/// Decide whether Mode B VM/container resolve may select `IosHv`.
pub fn probe_ios_hv(artifact: ArtifactClass, host: Option<&IosHvHost>) -> IosHvProbe {
    if artifact != ArtifactClass::ModeB {
        return IosHvProbe {
            supported: false,
            reason: IosHvReason::ModeA.as_str(),
            soc: None,
            target: None,
        };
    }
    let Some(host) = host else {
        return IosHvProbe {
            supported: false,
            reason: IosHvReason::NoHostFacts.as_str(),
            soc: None,
            target: None,
        };
    };
    if host.has_private_hypervisor_entitlement == Some(false) {
        return IosHvProbe {
            supported: false,
            reason: IosHvReason::MissingEntitlement.as_str(),
            soc: None,
            target: None,
        };
    }
    if host.kernel_hv == Some(false) {
        return IosHvProbe {
            supported: false,
            reason: IosHvReason::KernelRemoved.as_str(),
            soc: None,
            target: None,
        };
    }
    if is_a12z(&host.hw_machine) {
        return IosHvProbe {
            supported: false,
            reason: IosHvReason::SocNotPublic.as_str(),
            soc: None,
            target: Some("a12z"),
        };
    }
    let Some((target, soc, floor)) = public_hv_target(&host.hw_machine) else {
        return IosHvProbe {
            supported: false,
            reason: IosHvReason::SocUnknown.as_str(),
            soc: None,
            target: None,
        };
    };
    if !host.os.at_least(floor) {
        return IosHvProbe {
            supported: false,
            reason: IosHvReason::OsTooOld.as_str(),
            soc: Some(soc_name(soc)),
            target: Some(target),
        };
    }
    if !host.os.at_most(IOS_HV_OS_CEILING) {
        return IosHvProbe {
            supported: false,
            reason: IosHvReason::OsTooNew.as_str(),
            soc: Some(soc_name(soc)),
            target: Some(target),
        };
    }
    IosHvProbe {
        supported: true,
        reason: IosHvReason::Supported.as_str(),
        soc: Some(soc_name(soc)),
        target: Some(target),
    }
}

/// Read `hw.machine` and `kern.osproductversion` on an iOS process.
/// macOS / Linux / Android return `None` (those hosts are not this probe).
pub fn live_ios_hv_host() -> Option<IosHvHost> {
    #[cfg(target_os = "ios")]
    {
        let hw_machine = sysctl_string("hw.machine")?;
        let os = OsVersion::parse(&sysctl_string("kern.osproductversion")?)?;
        Some(IosHvHost {
            os,
            hw_machine,
            kernel_hv: live_kernel_hv(),
            has_private_hypervisor_entitlement: None,
        })
    }
    #[cfg(not(target_os = "ios"))]
    {
        None
    }
}

#[cfg(target_os = "ios")]
fn sysctl_string(name: &str) -> Option<String> {
    use std::ffi::CString;
    let key = CString::new(name).ok()?;
    let mut len = 0usize;
    let rc = unsafe {
        sysctlbyname(
            key.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    let rc = unsafe {
        sysctlbyname(
            key.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    if let Some(end) = buf.iter().position(|&b| b == 0) {
        buf.truncate(end);
    }
    String::from_utf8(buf).ok()
}

#[cfg(target_os = "ios")]
unsafe extern "C" {
    fn sysctlbyname(
        name: *const std::os::raw::c_char,
        oldp: *mut std::ffi::c_void,
        oldlenp: *mut usize,
        newp: *mut std::ffi::c_void,
        newlen: usize,
    ) -> std::os::raw::c_int;
}

/// Kernel trap is Mode B only. Store IPA must not enable `ios-hv`.
#[cfg(all(target_os = "ios", feature = "ios-hv"))]
fn live_kernel_hv() -> Option<bool> {
    Some(unsafe { hv_trap_supported() })
}

#[cfg(all(target_os = "ios", not(feature = "ios-hv")))]
fn live_kernel_hv() -> Option<bool> {
    None
}

/// UTM `jb_has_hypervisor`: `HV_CALL_VM_GET_CAPABILITIES` via `svc 0x80`.
/// `HV_UNSUPPORTED` (0xfae9400f) means the kernel has no HV.
#[cfg(all(target_os = "ios", target_arch = "aarch64", feature = "ios-hv"))]
unsafe fn hv_trap_supported() -> bool {
    const HV_CALL_VM_GET_CAPABILITIES: u64 = 0;
    const HV_UNSUPPORTED: i32 = 0x0fae_940f_u32 as i32;
    let status: i32;
    core::arch::asm!(
        "mov x16, #-0x5",
        "mov x0, {call}",
        "mov x1, xzr",
        "svc 0x80",
        call = in(reg) HV_CALL_VM_GET_CAPABILITIES,
        lateout("x0") status,
        lateout("x16") _,
        options(nostack, preserves_flags)
    );
    status != HV_UNSUPPORTED
}

#[cfg(all(target_os = "ios", feature = "ios-hv", not(target_arch = "aarch64")))]
unsafe fn hv_trap_supported() -> bool {
    false
}

/// macOS product VMs stay VZ. Lab binaries need `com.apple.security.hypervisor`.
pub const MACOS_HV_LAB_ENTITLEMENT: &str = "com.apple.security.hypervisor";

/// Nested virt and GitHub-hosted runners often return `HV_UNSUPPORTED`.
/// Skip. Do not fail the suite.
pub fn macos_hv_lab_skip_reason(hv_status: i32) -> Option<&'static str> {
    const HV_UNSUPPORTED: i32 = -5;
    const HV_DENIED: i32 = -4;
    match hv_status {
        HV_UNSUPPORTED => Some("nested-or-no-hv"),
        HV_DENIED => Some("missing-hypervisor-entitlement"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(machine: &str, major: u32, minor: u32, patch: u32) -> IosHvHost {
        IosHvHost {
            os: OsVersion::new(major, minor, patch),
            hw_machine: machine.into(),
            kernel_hv: Some(true),
            has_private_hypervisor_entitlement: Some(true),
        }
    }

    #[test]
    fn mode_a_never_hv() {
        let p = probe_ios_hv(
            ArtifactClass::ModeA,
            Some(&IosHvHost::iphone_14_pro_16_3_1()),
        );
        assert!(!p.supported);
        assert_eq!(p.reason, "mode-a");
    }

    #[test]
    fn mode_b_without_facts_stays_closed() {
        let p = probe_ios_hv(ArtifactClass::ModeB, None);
        assert!(!p.supported);
        assert_eq!(p.reason, "no-host-facts");
    }

    #[test]
    fn iphone_14_pro_16_3_1_is_supported() {
        let p = probe_ios_hv(
            ArtifactClass::ModeB,
            Some(&IosHvHost::iphone_14_pro_16_3_1()),
        );
        assert!(p.supported);
        assert_eq!(p.soc, Some("a16"));
        assert_eq!(p.target, Some("iphone-14-pro"));
    }

    #[test]
    fn ios_16_4_is_blocked() {
        let p = probe_ios_hv(ArtifactClass::ModeB, Some(&host("iPhone15,2", 16, 4, 0)));
        assert!(!p.supported);
        assert_eq!(p.reason, "os-too-new");
    }

    #[test]
    fn a15_iphone_14_is_blocked() {
        let p = probe_ios_hv(ArtifactClass::ModeB, Some(&host("iPhone14,7", 16, 3, 1)));
        assert!(!p.supported);
        assert_eq!(p.reason, "soc-unknown");
    }

    #[test]
    fn a12z_is_not_public() {
        let p = probe_ios_hv(ArtifactClass::ModeB, Some(&host("iPad8,11", 16, 3, 1)));
        assert!(!p.supported);
        assert_eq!(p.reason, "soc-not-public");
    }

    #[test]
    fn ipad_air_m1_needs_15_4() {
        let too_old = probe_ios_hv(ArtifactClass::ModeB, Some(&host("iPad13,16", 15, 3, 0)));
        assert_eq!(too_old.reason, "os-too-old");
        let ok = probe_ios_hv(ArtifactClass::ModeB, Some(&host("iPad13,16", 15, 4, 0)));
        assert!(ok.supported);
        assert_eq!(ok.soc, Some("m1"));
    }

    #[test]
    fn ipad_pro_m2_needs_16_1() {
        let too_old = probe_ios_hv(ArtifactClass::ModeB, Some(&host("iPad14,5", 16, 0, 3)));
        assert_eq!(too_old.reason, "os-too-old");
        let ok = probe_ios_hv(ArtifactClass::ModeB, Some(&host("iPad14,5", 16, 1, 0)));
        assert!(ok.supported);
        assert_eq!(ok.soc, Some("m2"));
    }

    #[test]
    fn kernel_removed_blocks_window_device() {
        let mut h = IosHvHost::iphone_14_pro_16_3_1();
        h.kernel_hv = Some(false);
        let p = probe_ios_hv(ArtifactClass::ModeB, Some(&h));
        assert_eq!(p.reason, "kernel-removed");
    }

    #[test]
    fn macos_lab_skip_does_not_fail_nested() {
        assert_eq!(macos_hv_lab_skip_reason(-5), Some("nested-or-no-hv"));
        assert_eq!(
            macos_hv_lab_skip_reason(-4),
            Some("missing-hypervisor-entitlement")
        );
        assert_eq!(macos_hv_lab_skip_reason(0), None);
    }
}

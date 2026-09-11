//! Relay spec and backend table.
//!
//! Wawona UI never picks a hypervisor. Relay resolves the backend: VZ on
//! macOS, KVM on Linux, `IosHv` on Mode B iOS/iPadOS when the host probe
//! matches the public HV window, otherwise static CPU. Wasm is Pulley on
//! Apple mobile regardless.

pub mod ios_hv;

pub use ios_hv::{live_ios_hv_host, probe_ios_hv, IosHvHost, IosHvProbe};

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::os::raw::c_int;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayKind {
    Vm,
    Container,
    Wasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayPlatform {
    Macos,
    Ios,
    Ipados,
    Tvos,
    Watchos,
    Visionos,
    Android,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayBackend {
    None,
    Vz,
    KvmCloudHypervisor,
    KvmCrosvm,
    AvfLab,
    StaticCpu,
    ModeBJit,
    IosHv,
    WasmPulley,
    WasmCranelift,
}

impl RelayBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Vz => "vz",
            Self::KvmCloudHypervisor => "kvm-ch",
            Self::KvmCrosvm => "kvm-crosvm",
            Self::AvfLab => "avf-lab",
            Self::StaticCpu => "static-cpu",
            Self::ModeBJit => "mode-b-jit",
            Self::IosHv => "ios-hv",
            Self::WasmPulley => "wasm-pulley",
            Self::WasmCranelift => "wasm-cranelift",
        }
    }

    pub fn is_qemu(self) -> bool {
        false
    }
}

/// Artifact class is which binary the user installed. Never an in-app toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactClass {
    ModeA,
    ModeB,
}

/// Versioned VM/container profile owned by Relay. Native views edit this
/// domain through generated bindings instead of duplicating policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayMachineProfile {
    pub version: u32,
    pub id: String,
    pub kind: RelayKind,
    pub guest: GuestManifest,
    #[serde(default)]
    pub image: Option<String>,
    pub memory_mb: u32,
    pub disk_gib: u32,
    pub max_disk_gib: u32,
}

impl RelayMachineProfile {
    pub const VERSION: u32 = 1;
    pub const MIN_MEMORY_MB: u32 = 256;
    pub const MAX_MEMORY_MB: u32 = 65_536;

    pub fn validate(&self) -> Result<(), RelayError> {
        if self.version != Self::VERSION {
            return Err(RelayError::Failed(format!(
                "unsupported Relay machine profile version {}",
                self.version
            )));
        }
        let id = self.id.trim();
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(RelayError::Failed(
                "machine profile id must be 1-128 ASCII identifier characters".into(),
            ));
        }
        if !matches!(self.kind, RelayKind::Vm | RelayKind::Container) {
            return Err(RelayError::Failed(
                "Relay machine profiles support VM or container kinds".into(),
            ));
        }
        if !(Self::MIN_MEMORY_MB..=Self::MAX_MEMORY_MB).contains(&self.memory_mb) {
            return Err(RelayError::Failed(
                "machine profile memory is outside Relay limits".into(),
            ));
        }
        let expected_memory = u64::from(self.memory_mb) * 1024 * 1024;
        if self.guest.memory_bytes != expected_memory {
            return Err(RelayError::Failed(
                "machine profile memory differs from guest manifest".into(),
            ));
        }
        self.guest.validate()?;
        DiskResizePlan::from_slider(self.disk_gib, self.disk_gib, self.max_disk_gib, false)?;
        if self.kind == RelayKind::Vm && self.image.is_some() {
            return Err(RelayError::Failed(
                "VM profile cannot contain an OCI image".into(),
            ));
        }
        if self.kind == RelayKind::Container
            && self
                .image
                .as_deref()
                .is_none_or(|image| image.trim().is_empty())
        {
            return Err(RelayError::Failed(
                "container profile requires an OCI image".into(),
            ));
        }
        Ok(())
    }

    pub fn start_spec(
        &self,
        platform: RelayPlatform,
        artifact: ArtifactClass,
    ) -> Result<RelaySpec, RelayError> {
        self.validate()?;
        Ok(RelaySpec {
            kind: self.kind,
            platform,
            artifact,
            machine_id: Some(self.id.clone()),
            image: self.image.clone(),
            memory_mb: Some(self.memory_mb),
            guest_page_size: Some(self.guest.page_size),
            guest: Some(self.guest.clone()),
            resources: None,
            ios_hv_host: None,
        })
    }
}

/// Page geometry used by the guest's MMU and virtio memory map. Relay accepts
/// only AArch64 Linux page sizes we build guests for. The absent JSON value is
/// deliberate: it selects the running device's page size, rather than baking
/// the macOS builder's geometry into an iOS archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GuestPageSize(pub u32);

impl GuestPageSize {
    pub const FOUR_KIB: Self = Self(4096);
    pub const SIXTEEN_KIB: Self = Self(16384);

    pub fn resolve(requested: Option<u32>) -> Result<Self, RelayError> {
        let bytes = requested.unwrap_or_else(host_page_size);
        match bytes {
            4096 => Ok(Self::FOUR_KIB),
            16384 => Ok(Self::SIXTEEN_KIB),
            other => Err(RelayError::Failed(format!(
                "Relay supports 4 KiB or 16 KiB pages, got {other}"
            ))),
        }
    }

    pub fn bytes(self) -> usize {
        self.0 as usize
    }

    pub fn is_aligned(self, address: u64) -> bool {
        address & (self.0 as u64 - 1) == 0
    }
}

/// Host process page size. On Apple mobile this is typically 16 KiB. Relay's
/// StaticCpu maps either guest geometry onto this host size without a second
/// VM product (see `relay_vm::page_translate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostPageSize(pub u32);

impl HostPageSize {
    pub const FOUR_KIB: Self = Self(4096);
    pub const SIXTEEN_KIB: Self = Self(16384);

    pub fn detect() -> Self {
        match host_page_size() {
            16384 => Self::SIXTEEN_KIB,
            _ => Self::FOUR_KIB,
        }
    }

    pub fn resolve(bytes: u32) -> Result<Self, RelayError> {
        match bytes {
            4096 => Ok(Self::FOUR_KIB),
            16384 => Ok(Self::SIXTEEN_KIB),
            other => Err(RelayError::Failed(format!(
                "Relay host page size must be 4 KiB or 16 KiB, got {other}"
            ))),
        }
    }

    pub fn bytes(self) -> usize {
        self.0 as usize
    }

    pub fn is_aligned(self, address: u64) -> bool {
        address & (self.0 as u64 - 1) == 0
    }
}

#[cfg(unix)]
fn host_page_size() -> u32 {
    unsafe extern "C" {
        fn getpagesize() -> c_int;
    }
    let bytes = unsafe { getpagesize() };
    u32::try_from(bytes).unwrap_or(4096)
}

#[cfg(not(unix))]
fn host_page_size() -> u32 {
    4096
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaySpec {
    pub kind: RelayKind,
    pub platform: RelayPlatform,
    #[serde(default)]
    pub artifact: ArtifactClass,
    #[serde(default)]
    pub machine_id: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub memory_mb: Option<u32>,
    /// `None` selects host page geometry at runtime. Do not serialize the
    /// builder host page size into an iOS machine profile.
    #[serde(default)]
    pub guest_page_size: Option<u32>,
    /// A verified Linux guest description. A missing manifest selects the
    /// small static-CPU proof only; it never means that a Linux VM booted.
    #[serde(default)]
    pub guest: Option<GuestManifest>,
    /// Runtime paths supplied by native packaging. They are not persisted in
    /// machine profiles and never select the VM backend.
    #[serde(default)]
    pub resources: Option<RelayRuntimeResources>,
    /// Injected host facts for tests and for L4 on a Mac targeting an iOS
    /// spec. A Mode B iOS process fills this from sysctl when it is `None`.
    #[serde(default)]
    pub ios_hv_host: Option<IosHvHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayRuntimeResources {
    #[serde(default)]
    pub launcher: String,
    #[serde(default)]
    pub state_directory: String,
    #[serde(default)]
    pub guest_directory: Option<String>,
    #[serde(default)]
    pub trusted_guest_keys: BTreeMap<String, String>,
    #[serde(default)]
    pub allow_unsigned_guest: bool,
}

/// One immutable file named by a guest manifest. Hashes are metadata until
/// `relay-vm` verifies them against the bundle before a real boot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestArtifact {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Versioned NixOS guest input. Keep guest image selection outside the UI so
/// every platform consumes the same signed, page-size-specific record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestManifest {
    pub version: u32,
    pub page_size: u32,
    pub memory_bytes: u64,
    pub kernel: GuestArtifact,
    #[serde(default)]
    pub initrd: Option<GuestArtifact>,
    pub rootfs: GuestArtifact,
    #[serde(default)]
    pub command_line: String,
    #[serde(default)]
    pub compatibility: GuestCompatibility,
    #[serde(default)]
    pub signature: Option<GuestManifestSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestCompatibility {
    pub architecture: String,
    pub os: String,
    pub minimum_relay_version: String,
    #[serde(default)]
    pub kernel_release: Option<String>,
}

impl Default for GuestCompatibility {
    fn default() -> Self {
        Self {
            architecture: "aarch64".into(),
            os: "linux".into(),
            minimum_relay_version: env!("CARGO_PKG_VERSION").into(),
            kernel_release: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestManifestSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

/// Pure Relay domain result consumed by native disk-size sliders. Values are
/// GiB so the UI has predictable discrete steps on phone and desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskResizePlan {
    pub current_gib: u32,
    pub target_gib: u32,
}

impl DiskResizePlan {
    pub const MIN_GIB: u32 = 4;

    pub fn from_slider(
        current_gib: u32,
        target_gib: u32,
        max_gib: u32,
        running: bool,
    ) -> Result<Self, RelayError> {
        if running {
            return Err(RelayError::Failed(
                "stop the MicroVM before resizing its disk".into(),
            ));
        }
        if current_gib < Self::MIN_GIB || target_gib < Self::MIN_GIB || max_gib < Self::MIN_GIB {
            return Err(RelayError::Failed(
                "Relay virtual disks require at least 4 GiB".into(),
            ));
        }
        if target_gib < current_gib {
            return Err(RelayError::Failed(
                "Relay virtual disks are grow-only".into(),
            ));
        }
        if target_gib > max_gib {
            return Err(RelayError::Failed(
                "requested disk size exceeds machine quota".into(),
            ));
        }
        Ok(Self {
            current_gib,
            target_gib,
        })
    }

    pub fn grows(self) -> bool {
        self.target_gib > self.current_gib
    }
}

impl GuestManifest {
    pub const VERSION: u32 = 1;

    pub fn validate(&self) -> Result<GuestPageSize, RelayError> {
        if self.version != Self::VERSION {
            return Err(RelayError::Failed(format!(
                "unsupported Relay guest manifest version {}",
                self.version
            )));
        }
        let page_size = GuestPageSize::resolve(Some(self.page_size))?;
        if self.memory_bytes < (page_size.bytes() as u64) * 16
            || !page_size.is_aligned(self.memory_bytes)
        {
            return Err(RelayError::Failed(
                "guest memory must be at least 16 pages and page aligned".into(),
            ));
        }
        validate_artifact("kernel", &self.kernel)?;
        validate_artifact("rootfs", &self.rootfs)?;
        if let Some(initrd) = &self.initrd {
            validate_artifact("initrd", initrd)?;
        }
        if self.command_line.contains('\0') {
            return Err(RelayError::Failed("guest command line contains NUL".into()));
        }
        if self.compatibility.architecture != "aarch64"
            || self.compatibility.os != "linux"
            || self.compatibility.minimum_relay_version.trim().is_empty()
            || self
                .compatibility
                .kernel_release
                .as_deref()
                .is_some_and(|release| release.trim().is_empty())
        {
            return Err(RelayError::Failed(
                "guest compatibility metadata is unsupported".into(),
            ));
        }
        Ok(page_size)
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>, RelayError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        serde_json::to_vec(&unsigned)
            .map_err(|error| RelayError::Failed(format!("cannot encode guest manifest: {error}")))
    }

    pub fn verify_trust(
        &self,
        trusted_keys: &BTreeMap<String, String>,
        allow_unsigned: bool,
    ) -> Result<(), RelayError> {
        let Some(signature) = &self.signature else {
            return if allow_unsigned {
                Ok(())
            } else {
                Err(RelayError::Failed(
                    "guest manifest requires a trusted signature".into(),
                ))
            };
        };
        if signature.algorithm != "ed25519" {
            return Err(RelayError::Failed(
                "guest manifest signature algorithm is unsupported".into(),
            ));
        }
        let public_key = trusted_keys.get(&signature.key_id).ok_or_else(|| {
            RelayError::Failed("guest manifest signing key is not trusted".into())
        })?;
        let public_key = decode_hex::<32>(public_key, "guest public key")?;
        let signature_bytes = decode_hex::<64>(&signature.value, "guest signature")?;
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .map_err(|_| RelayError::Failed("guest public key is invalid".into()))?;
        verifying_key
            .verify_strict(
                &self.signing_bytes()?,
                &Signature::from_bytes(&signature_bytes),
            )
            .map_err(|_| RelayError::Failed("guest manifest signature is invalid".into()))
    }
}

fn decode_hex<const N: usize>(input: &str, name: &str) -> Result<[u8; N], RelayError> {
    if input.len() != N * 2 {
        return Err(RelayError::Failed(format!("{name} has invalid length")));
    }
    let mut output = [0u8; N];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&input[index * 2..index * 2 + 2], 16)
            .map_err(|_| RelayError::Failed(format!("{name} is not hexadecimal")))?;
    }
    Ok(output)
}

fn validate_artifact(name: &str, artifact: &GuestArtifact) -> Result<(), RelayError> {
    if artifact.path.is_empty() || artifact.bytes == 0 {
        return Err(RelayError::Failed(format!("guest {name} is empty")));
    }
    if artifact.sha256.len() != 64 || !artifact.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(RelayError::Failed(format!(
            "guest {name} has invalid sha256"
        )));
    }
    Ok(())
}

impl Default for ArtifactClass {
    fn default() -> Self {
        Self::ModeA
    }
}

#[derive(Debug, Error)]
pub enum RelayError {
    #[error("{0}")]
    Forbidden(&'static str),
    #[error("{0}")]
    Planned(&'static str),
    #[error("{0}")]
    Failed(String),
}

impl RelayError {
    pub fn qemu() -> Self {
        Self::Forbidden("QEMU and UTM are not Relay backends")
    }
}

/// Resolve `(platform, artifact, kind)` to one backend. Never QEMU.
pub fn resolve_backend(spec: &RelaySpec) -> Result<RelayBackend, RelayError> {
    match spec.kind {
        RelayKind::Wasm => resolve_wasm(spec),
        RelayKind::Vm | RelayKind::Container => resolve_linux_vm(spec),
    }
}

fn resolve_wasm(spec: &RelaySpec) -> Result<RelayBackend, RelayError> {
    // No Mode B wasm catalog. Same /wasm/v1 bytecode. Mode B may execute
    // it (Pulley on Apple mobile; Cranelift only where Mode A already does).
    // Android Play Mode A stays Pulley (store-safe). Mode B Android may use
    // Cranelift.
    match spec.platform {
        RelayPlatform::Macos | RelayPlatform::Linux => Ok(RelayBackend::WasmCranelift),
        RelayPlatform::Android => match spec.artifact {
            ArtifactClass::ModeA => Ok(RelayBackend::WasmPulley),
            ArtifactClass::ModeB => Ok(RelayBackend::WasmCranelift),
        },
        RelayPlatform::Ios
        | RelayPlatform::Ipados
        | RelayPlatform::Tvos
        | RelayPlatform::Watchos
        | RelayPlatform::Visionos => Ok(RelayBackend::WasmPulley),
    }
}

fn resolve_linux_vm(spec: &RelaySpec) -> Result<RelayBackend, RelayError> {
    match spec.platform {
        RelayPlatform::Tvos | RelayPlatform::Watchos | RelayPlatform::Visionos => {
            Err(RelayError::Forbidden(
                "VM and container machine kinds are forbidden on tvOS/watchOS/visionOS",
            ))
        }
        RelayPlatform::Macos => Ok(RelayBackend::Vz),
        RelayPlatform::Linux => Ok(RelayBackend::KvmCloudHypervisor),
        RelayPlatform::Ios | RelayPlatform::Ipados => Ok(resolve_ios_linux_vm(spec)),
        RelayPlatform::Android => match spec.artifact {
            ArtifactClass::ModeA => Err(RelayError::Planned(
                "Android Play Linux VMs wait on Relay static CPU. No AVF, no QEMU, no proot",
            )),
            ArtifactClass::ModeB => Ok(RelayBackend::AvfLab),
        },
    }
}

fn resolve_ios_linux_vm(spec: &RelaySpec) -> RelayBackend {
    match spec.artifact {
        ArtifactClass::ModeA => RelayBackend::StaticCpu,
        ArtifactClass::ModeB => {
            let live = live_ios_hv_host();
            let host = spec.ios_hv_host.as_ref().or(live.as_ref());
            if probe_ios_hv(spec.artifact, host).supported {
                RelayBackend::IosHv
            } else {
                // MAP_JIT JIT CPU is a later Mode B path. Do not advertise it
                // until write+exec is real. HV is the windowed Mode B VM.
                RelayBackend::StaticCpu
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: RelayKind, platform: RelayPlatform, artifact: ArtifactClass) -> RelaySpec {
        RelaySpec {
            kind,
            platform,
            artifact,
            machine_id: None,
            image: None,
            memory_mb: None,
            guest_page_size: None,
            guest: None,
            resources: None,
            ios_hv_host: None,
        }
    }

    #[test]
    fn macos_vm_is_vz() {
        let b = resolve_backend(&spec(
            RelayKind::Vm,
            RelayPlatform::Macos,
            ArtifactClass::ModeA,
        ))
        .unwrap();
        assert_eq!(b, RelayBackend::Vz);
        assert!(!b.is_qemu());
    }

    #[test]
    fn linux_vm_is_kvm() {
        let b = resolve_backend(&spec(
            RelayKind::Vm,
            RelayPlatform::Linux,
            ArtifactClass::ModeA,
        ))
        .unwrap();
        assert_eq!(b, RelayBackend::KvmCloudHypervisor);
    }

    #[test]
    fn container_uses_same_vm_backend() {
        let vm = resolve_backend(&spec(
            RelayKind::Vm,
            RelayPlatform::Macos,
            ArtifactClass::ModeA,
        ))
        .unwrap();
        let c = resolve_backend(&spec(
            RelayKind::Container,
            RelayPlatform::Macos,
            ArtifactClass::ModeA,
        ))
        .unwrap();
        assert_eq!(vm, c);
    }

    #[test]
    fn watch_vm_forbidden() {
        assert!(matches!(
            resolve_backend(&spec(
                RelayKind::Vm,
                RelayPlatform::Watchos,
                ArtifactClass::ModeA
            )),
            Err(RelayError::Forbidden(_))
        ));
    }

    #[test]
    fn ios_mode_a_vm_is_static_cpu() {
        let b = resolve_backend(&spec(
            RelayKind::Vm,
            RelayPlatform::Ios,
            ArtifactClass::ModeA,
        ))
        .unwrap();
        assert_eq!(b, RelayBackend::StaticCpu);
        assert!(!b.is_qemu());
    }

    #[test]
    fn ios_mode_b_vm_stays_static_without_host_facts() {
        let b = resolve_backend(&spec(
            RelayKind::Vm,
            RelayPlatform::Ios,
            ArtifactClass::ModeB,
        ))
        .unwrap();
        assert_eq!(b, RelayBackend::StaticCpu);
        assert!(!b.is_qemu());
    }

    #[test]
    fn ios_mode_b_window_device_selects_ios_hv() {
        let mut s = spec(RelayKind::Vm, RelayPlatform::Ios, ArtifactClass::ModeB);
        s.ios_hv_host = Some(IosHvHost::iphone_14_pro_16_3_1());
        let b = resolve_backend(&s).unwrap();
        assert_eq!(b, RelayBackend::IosHv);
        assert!(!b.is_qemu());
    }

    #[test]
    fn ios_mode_b_container_uses_same_hv_backend() {
        let mut vm = spec(RelayKind::Vm, RelayPlatform::Ipados, ArtifactClass::ModeB);
        vm.ios_hv_host = Some(IosHvHost::iphone_14_pro_16_3_1());
        let mut c = spec(
            RelayKind::Container,
            RelayPlatform::Ipados,
            ArtifactClass::ModeB,
        );
        c.ios_hv_host = vm.ios_hv_host.clone();
        assert_eq!(resolve_backend(&vm).unwrap(), RelayBackend::IosHv);
        assert_eq!(resolve_backend(&c).unwrap(), RelayBackend::IosHv);
    }

    #[test]
    fn ios_mode_a_never_selects_ios_hv_on_window_device() {
        let mut s = spec(RelayKind::Vm, RelayPlatform::Ios, ArtifactClass::ModeA);
        s.ios_hv_host = Some(IosHvHost::iphone_14_pro_16_3_1());
        assert_eq!(resolve_backend(&s).unwrap(), RelayBackend::StaticCpu);
    }

    #[test]
    fn ios_mode_b_16_4_stays_static() {
        let mut s = spec(RelayKind::Vm, RelayPlatform::Ios, ArtifactClass::ModeB);
        s.ios_hv_host = Some(IosHvHost {
            os: ios_hv::OsVersion::new(16, 4, 0),
            hw_machine: "iPhone15,2".into(),
            kernel_hv: Some(true),
            has_private_hypervisor_entitlement: Some(true),
        });
        assert_eq!(resolve_backend(&s).unwrap(), RelayBackend::StaticCpu);
    }

    #[test]
    fn ios_mode_b_wasm_is_pulley() {
        let b = resolve_backend(&spec(
            RelayKind::Wasm,
            RelayPlatform::Ios,
            ArtifactClass::ModeB,
        ))
        .unwrap();
        assert_eq!(b, RelayBackend::WasmPulley);
    }

    #[test]
    fn ios_mode_b_wasm_stays_pulley_on_hv_window_device() {
        let mut s = spec(RelayKind::Wasm, RelayPlatform::Ios, ArtifactClass::ModeB);
        s.ios_hv_host = Some(IosHvHost::iphone_14_pro_16_3_1());
        assert_eq!(resolve_backend(&s).unwrap(), RelayBackend::WasmPulley);
    }

    #[test]
    fn android_mode_a_wasm_is_pulley() {
        let b = resolve_backend(&spec(
            RelayKind::Wasm,
            RelayPlatform::Android,
            ArtifactClass::ModeA,
        ))
        .unwrap();
        assert_eq!(b, RelayBackend::WasmPulley);
    }

    #[test]
    fn apple_mobile_wasm_is_pulley() {
        let b = resolve_backend(&spec(
            RelayKind::Wasm,
            RelayPlatform::Watchos,
            ArtifactClass::ModeA,
        ))
        .unwrap();
        assert_eq!(b, RelayBackend::WasmPulley);
    }

    #[test]
    fn relay_accepts_both_linux_page_sizes() {
        assert_eq!(GuestPageSize::resolve(Some(4096)).unwrap().bytes(), 4096);
        assert_eq!(GuestPageSize::resolve(Some(16384)).unwrap().bytes(), 16384);
    }

    #[test]
    fn relay_rejects_unknown_page_size() {
        assert!(GuestPageSize::resolve(Some(65536)).is_err());
    }

    #[test]
    fn guest_manifest_requires_valid_linux_geometry() {
        let artifact = GuestArtifact {
            path: "nixos/Image".into(),
            sha256: "a".repeat(64),
            bytes: 4096,
        };
        let manifest = GuestManifest {
            version: GuestManifest::VERSION,
            page_size: 4096,
            memory_bytes: 4096 * 16,
            kernel: artifact.clone(),
            initrd: None,
            rootfs: artifact,
            command_line: "console=hvc0".into(),
            compatibility: GuestCompatibility::default(),
            signature: None,
        };
        assert_eq!(manifest.validate().unwrap(), GuestPageSize::FOUR_KIB);
    }

    #[test]
    fn guest_manifest_requires_a_trusted_ed25519_signature_in_release_mode() {
        use ed25519_dalek::{Signer, SigningKey};

        let artifact = GuestArtifact {
            path: "nixos/Image".into(),
            sha256: "a".repeat(64),
            bytes: 4096,
        };
        let mut manifest = GuestManifest {
            version: GuestManifest::VERSION,
            page_size: 4096,
            memory_bytes: 4096 * 16,
            kernel: artifact.clone(),
            initrd: None,
            rootfs: artifact,
            command_line: "console=hvc0".into(),
            compatibility: GuestCompatibility::default(),
            signature: None,
        };
        assert!(manifest.verify_trust(&BTreeMap::new(), false).is_err());
        assert!(manifest.verify_trust(&BTreeMap::new(), true).is_ok());

        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let signature = signing_key.sign(&manifest.signing_bytes().unwrap());
        manifest.signature = Some(GuestManifestSignature {
            algorithm: "ed25519".into(),
            key_id: "test-release".into(),
            value: signature
                .to_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        });
        let mut trusted = BTreeMap::new();
        trusted.insert(
            "test-release".into(),
            signing_key
                .verifying_key()
                .to_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        );
        manifest.verify_trust(&trusted, false).unwrap();
        manifest.memory_bytes += 4096;
        assert!(manifest.verify_trust(&trusted, false).is_err());
    }

    #[test]
    fn microvm_disk_slider_only_allows_stopped_growth() {
        let plan = DiskResizePlan::from_slider(8, 16, 64, false).unwrap();
        assert!(plan.grows());
        assert!(DiskResizePlan::from_slider(16, 8, 64, false).is_err());
        assert!(DiskResizePlan::from_slider(8, 16, 64, true).is_err());
    }

    #[test]
    fn versioned_machine_profile_owns_vm_and_container_policy() {
        let artifact = GuestArtifact {
            path: "guest/Image".into(),
            sha256: "a".repeat(64),
            bytes: 4096,
        };
        let guest = GuestManifest {
            version: GuestManifest::VERSION,
            page_size: 4096,
            memory_bytes: 512 * 1024 * 1024,
            kernel: artifact.clone(),
            initrd: None,
            rootfs: artifact,
            command_line: "console=hvc0".into(),
            compatibility: GuestCompatibility::default(),
            signature: None,
        };
        let vm = RelayMachineProfile {
            version: RelayMachineProfile::VERSION,
            id: "nixos-main".into(),
            kind: RelayKind::Vm,
            guest: guest.clone(),
            image: None,
            memory_mb: 512,
            disk_gib: 8,
            max_disk_gib: 64,
        };
        let spec = vm
            .start_spec(RelayPlatform::Macos, ArtifactClass::ModeA)
            .unwrap();
        assert_eq!(spec.kind, RelayKind::Vm);
        assert_eq!(spec.guest_page_size, Some(4096));

        let mut container = vm.clone();
        container.kind = RelayKind::Container;
        assert!(container.validate().is_err());
        container.image = Some("registry.example/wawona:latest".into());
        assert!(container.validate().is_ok());
        container.kind = RelayKind::Wasm;
        assert!(container.validate().is_err());
    }
}

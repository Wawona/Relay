//! Relay spec and backend table. Wawona never picks a hypervisor.

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaySpec {
    pub kind: RelayKind,
    pub platform: RelayPlatform,
    #[serde(default)]
    pub artifact: ArtifactClass,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub memory_mb: Option<u32>,
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
    if spec.artifact == ArtifactClass::ModeB {
        return Err(RelayError::Forbidden(
            "No Mode B wasm product. Same /wasm/v1 bytecode. Mode B runs Linux VMs",
        ));
    }
    match spec.platform {
        RelayPlatform::Macos | RelayPlatform::Linux | RelayPlatform::Android => {
            Ok(RelayBackend::WasmCranelift)
        }
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
        RelayPlatform::Ios | RelayPlatform::Ipados => match spec.artifact {
            ArtifactClass::ModeA => Err(RelayError::Planned(
                "iOS Mode A Linux VMs wait on Relay static CPU. No QEMU",
            )),
            ArtifactClass::ModeB => Err(RelayError::Planned(
                "iOS Mode B Linux VMs wait on Relay JIT CPU. No QEMU",
            )),
        },
        RelayPlatform::Android => match spec.artifact {
            ArtifactClass::ModeA => Err(RelayError::Planned(
                "Android Play Linux VMs wait on Relay static CPU. No AVF, no QEMU, no proot",
            )),
            ArtifactClass::ModeB => Ok(RelayBackend::AvfLab),
        },
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
            image: None,
            memory_mb: None,
        }
    }

    #[test]
    fn macos_vm_is_vz() {
        let b = resolve_backend(&spec(RelayKind::Vm, RelayPlatform::Macos, ArtifactClass::ModeA))
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
    fn ios_mode_a_vm_planned_not_qemu() {
        let err = resolve_backend(&spec(RelayKind::Vm, RelayPlatform::Ios, ArtifactClass::ModeA))
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No QEMU"));
        assert!(!msg.to_lowercase().contains("tcti"));
    }

    #[test]
    fn no_mode_b_wasm_product() {
        assert!(matches!(
            resolve_backend(&spec(
                RelayKind::Wasm,
                RelayPlatform::Ios,
                ArtifactClass::ModeB
            )),
            Err(RelayError::Forbidden(_))
        ));
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
}

//! Linux VM start/stop. Containers always sit on this backend.

use relay_core::{RelayBackend, RelayError, RelayKind, RelaySpec, resolve_backend};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct RelayHandle {
    pub id: String,
    pub backend: RelayBackend,
    pub kind: RelayKind,
    pub wayland_endpoint: Option<String>,
}

pub fn start(spec: &RelaySpec) -> Result<RelayHandle, RelayError> {
    let backend = resolve_backend(spec)?;
    match backend {
        RelayBackend::Vz => start_vz(spec),
        RelayBackend::KvmCloudHypervisor | RelayBackend::KvmCrosvm => start_kvm(spec, backend),
        RelayBackend::AvfLab => Err(RelayError::Planned(
            "Android AVF is lab/root only. Not wired into Play",
        )),
        RelayBackend::StaticCpu | RelayBackend::ModeBJit => Err(RelayError::Planned(
            "Relay CPU cannot boot NixOS yet. Fail closed. No QEMU",
        )),
        RelayBackend::WasmPulley | RelayBackend::WasmCranelift | RelayBackend::None => {
            Err(RelayError::Failed("vm crate does not start wasm".into()))
        }
    }
}

fn start_vz(spec: &RelaySpec) -> Result<RelayHandle, RelayError> {
    if spec.kind == RelayKind::Wasm {
        return Err(RelayError::Failed("vz is a VM backend".into()));
    }
    // Process handoff to the imported VZ launcher happens in L4 until the
    // Swift Containerization / VZ FFI lands in this crate.
    Ok(handle(spec, RelayBackend::Vz, None))
}

fn start_kvm(spec: &RelaySpec, backend: RelayBackend) -> Result<RelayHandle, RelayError> {
    if !std::path::Path::new("/dev/kvm").exists() {
        return Err(RelayError::Failed(
            "/dev/kvm missing. Fail closed. No QEMU TCG".into(),
        ));
    }
    Ok(handle(spec, backend, None))
}

fn handle(spec: &RelaySpec, backend: RelayBackend, endpoint: Option<String>) -> RelayHandle {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    RelayHandle {
        id: format!("relay-{}-{}", backend.as_str(), n),
        backend,
        kind: spec.kind,
        wayland_endpoint: endpoint,
    }
}

pub fn stop(_handle: &str) -> Result<(), RelayError> {
    Ok(())
}

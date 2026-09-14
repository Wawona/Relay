//! Linux VM start/stop. Containers always sit on this backend.

mod bus;
mod cpu;
mod dtb;
mod exception;
mod guest;
mod linux_boot;
mod mmu;
pub mod page_translate;
mod sysregs;
mod timer;
mod virtio_block;
mod virtio_console;
mod virtio_mmio;
mod vsock;

pub use guest::GuestMemory;
pub use page_translate::PageTranslate;

use relay_core::{
    resolve_backend, GuestArtifact, GuestManifest, RelayBackend, RelayError, RelayKind,
    RelayRuntimeResources, RelaySpec,
};

#[cfg(target_os = "macos")]
use std::{
    collections::BTreeMap,
    ffi::CString,
    fs::{self, OpenOptions},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

/// Guest waypipe listens on this vsock port (`guest.nix` `vsockPort`, also
/// vfkit / microvm convention). Host dials the same port via `--vsock-connect`.
#[cfg(target_os = "macos")]
pub const WAYPIPE_VSOCK_PORT: u32 = 1024;

/// Virtiofs tag for OCI runtime bundles shared into the guest.
#[cfg(target_os = "macos")]
pub const OCI_SHARE_TAG: &str = "oci-bundle";

#[cfg(target_os = "macos")]
static NEXT_VZ_SESSION: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "macos")]
static VZ_SESSIONS: OnceLock<Mutex<BTreeMap<String, VzSession>>> = OnceLock::new();

#[cfg(target_os = "macos")]
struct VzSession {
    child: Child,
    machine_id: String,
    endpoint: PathBuf,
    log_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayVmStatus {
    Running,
    Exited,
}

#[derive(Debug, Clone)]
pub struct RelayHandle {
    pub id: String,
    pub backend: RelayBackend,
    pub kind: RelayKind,
    pub wayland_endpoint: Option<String>,
    pub frame: Option<Vec<u8>>,
    pub frame_width: u32,
    pub frame_height: u32,
}

pub fn start(spec: &RelaySpec) -> Result<RelayHandle, RelayError> {
    let backend = resolve_backend(spec)?;
    match backend {
        RelayBackend::Vz => start_vz(spec),
        RelayBackend::KvmCloudHypervisor | RelayBackend::KvmCrosvm => start_kvm(spec, backend),
        RelayBackend::AvfLab => Err(RelayError::Planned(
            "Android AVF is lab/root only. Not wired into Play",
        )),
        RelayBackend::StaticCpu | RelayBackend::ModeBJit => start_ios(spec),
        RelayBackend::IosHv => start_ios_hv(spec),
        RelayBackend::WasmPulley | RelayBackend::WasmCranelift | RelayBackend::None => {
            Err(RelayError::Failed("vm crate does not start wasm".into()))
        }
    }
}

fn start_ios(spec: &RelaySpec) -> Result<RelayHandle, RelayError> {
    if !matches!(spec.kind, RelayKind::Vm | RelayKind::Container) {
        return Err(RelayError::Failed(
            "static CPU is a guest-machine backend".into(),
        ));
    }
    let manifest = spec.guest.as_ref().ok_or_else(|| {
        RelayError::Failed("Relay VM/container start requires a guest manifest".into())
    })?;
    let resources = spec
        .resources
        .as_ref()
        .ok_or_else(|| RelayError::Failed("Relay guest trust configuration is missing".into()))?;
    manifest.verify_trust(
        &resources.trusted_guest_keys,
        resources.allow_unsigned_guest,
    )?;
    guest::validate_artifacts(manifest)?;
    linux_boot::prepare(manifest)?;
    unreachable!("linux_boot::prepare reports execution state or an error")
}

fn start_ios_hv(spec: &RelaySpec) -> Result<RelayHandle, RelayError> {
    if let Some(manifest) = &spec.guest {
        let resources = spec.resources.as_ref().ok_or_else(|| {
            RelayError::Failed("Relay guest trust configuration is missing".into())
        })?;
        manifest.verify_trust(
            &resources.trusted_guest_keys,
            resources.allow_unsigned_guest,
        )?;
        guest::validate_artifacts(manifest)?;
    }
    Err(RelayError::Planned(
        "Relay selected IosHv; Hypervisor.framework vCPU is not implemented yet",
    ))
}

fn start_vz(spec: &RelaySpec) -> Result<RelayHandle, RelayError> {
    if spec.kind == RelayKind::Wasm {
        return Err(RelayError::Failed("vz is a VM backend".into()));
    }
    #[cfg(target_os = "macos")]
    {
        return start_vz_macos(spec);
    }
    #[cfg(not(target_os = "macos"))]
    Err(RelayError::Failed(
        "Virtualization.framework is available only on macOS".into(),
    ))
}

#[cfg(target_os = "macos")]
fn start_vz_macos(spec: &RelaySpec) -> Result<RelayHandle, RelayError> {
    let resources = spec
        .resources
        .as_ref()
        .ok_or_else(|| RelayError::Failed("Relay VZ runtime resources are missing".into()))?;
    validate_resources(resources)?;
    let source_manifest = spec
        .guest
        .as_ref()
        .ok_or_else(|| RelayError::Failed("Relay VZ requires a guest manifest".into()))?;
    source_manifest.verify_trust(
        &resources.trusted_guest_keys,
        resources.allow_unsigned_guest,
    )?;
    let manifest = resolved_manifest(source_manifest, resources.guest_directory.as_deref())?;
    guest::validate_artifacts(&manifest)?;
    let initrd = manifest
        .initrd
        .as_ref()
        .ok_or_else(|| RelayError::Failed("Relay VZ requires a guest initrd".into()))?;
    let machine_id = machine_state_key(spec.machine_id.as_deref())?;
    recover_stale_session(&machine_id)?;

    let id = format!(
        "relay-vz-{}-{}",
        std::process::id(),
        NEXT_VZ_SESSION.fetch_add(1, Ordering::Relaxed)
    );
    let session_directory = Path::new(&resources.state_directory)
        .join("machines")
        .join(&machine_id);
    fs::create_dir_all(&session_directory).map_err(|error| {
        RelayError::Failed(format!("cannot create VM state directory: {error}"))
    })?;
    let disk = session_directory.join("rootfs.img");
    prepare_writable_disk(
        Path::new(&manifest.rootfs.path),
        &disk,
        manifest.rootfs.bytes,
    )?;
    let endpoint = session_directory.join("wayland.sock");
    if endpoint.as_os_str().as_bytes().len() >= 100 {
        return Err(RelayError::Failed(
            "Relay Wayland endpoint exceeds the Unix socket path limit".into(),
        ));
    }
    let log_path = session_directory.join("console.log");
    let _ = fs::remove_file(&endpoint);
    let log = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log_path)
        .map_err(|error| RelayError::Failed(format!("cannot create VM console log: {error}")))?;
    let stderr = log
        .try_clone()
        .map_err(|error| RelayError::Failed(format!("cannot clone VM console log: {error}")))?;
    let memory_mib = manifest.memory_bytes / (1024 * 1024);
    let mut command = Command::new(&resources.launcher);
    command
        .arg("--kernel")
        .arg(&manifest.kernel.path)
        .arg("--initrd")
        .arg(&initrd.path)
        .arg("--disk")
        .arg(&disk)
        .arg("--cmdline")
        .arg(&manifest.command_line)
        .arg("--memory-mib")
        .arg(memory_mib.to_string())
        .arg("--vsock-connect")
        .arg(WAYPIPE_VSOCK_PORT.to_string())
        .arg("--listen-unix")
        .arg(&endpoint);
    // Container kind: share a host-side OCI runtime bundle (rootfs +
    // config.json) into the guest over virtiofs. The guest mounts tag
    // `oci-bundle` and runs crun + waypipe when the share is present.
    if spec.kind == RelayKind::Container {
        let bundle = spec.image.as_deref().ok_or_else(|| {
            RelayError::Failed("Relay container start requires a materialized OCI bundle".into())
        })?;
        let bundle_path = Path::new(bundle);
        if !bundle_path.join("config.json").is_file() || !bundle_path.join("rootfs").is_dir() {
            return Err(RelayError::Failed(
                "Relay OCI share is not a runtime bundle (need config.json + rootfs/)".into(),
            ));
        }
        command
            .arg("--share-dir")
            .arg(bundle_path)
            .arg("--share-tag")
            .arg(OCI_SHARE_TAG);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| RelayError::Failed(format!("cannot start Relay VZ launcher: {error}")))?;

    if let Err(error) = wait_for_guest_ready(&mut child, &log_path, Duration::from_secs(90)) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let sessions = VZ_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()));
    sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay VZ session registry is poisoned".into()))?
        .insert(
            id.clone(),
            VzSession {
                child,
                machine_id,
                endpoint: endpoint.clone(),
                log_path,
            },
        );
    Ok(RelayHandle {
        id,
        backend: RelayBackend::Vz,
        kind: spec.kind,
        wayland_endpoint: Some(endpoint.display().to_string()),
        frame: None,
        frame_width: 0,
        frame_height: 0,
    })
}

#[cfg(target_os = "macos")]
fn machine_state_key(machine_id: Option<&str>) -> Result<String, RelayError> {
    let Some(machine_id) = machine_id else {
        return Ok(format!(
            "ephemeral-{}",
            NEXT_VZ_SESSION.load(Ordering::Relaxed)
        ));
    };
    if machine_id.is_empty()
        || machine_id.len() > 128
        || !machine_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(RelayError::Failed("Relay machine id is invalid".into()));
    }
    Ok(machine_id.to_string())
}

#[cfg(target_os = "macos")]
fn recover_stale_session(machine_id: &str) -> Result<(), RelayError> {
    let sessions = VZ_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay VZ session registry is poisoned".into()))?;
    let existing = sessions
        .iter_mut()
        .find(|(_, session)| session.machine_id == machine_id)
        .map(|(id, session)| (id.clone(), session.child.try_wait()));
    match existing {
        Some((_, Ok(None))) => Err(RelayError::Failed(
            "Relay machine is already running".into(),
        )),
        Some((id, Ok(Some(_)))) => {
            if let Some(session) = sessions.remove(&id) {
                let _ = fs::remove_file(session.endpoint);
            }
            Ok(())
        }
        Some((_, Err(error))) => Err(RelayError::Failed(format!(
            "cannot inspect existing Relay VM: {error}"
        ))),
        None => Ok(()),
    }
}

#[cfg(target_os = "macos")]
fn validate_resources(resources: &RelayRuntimeResources) -> Result<(), RelayError> {
    let launcher = Path::new(&resources.launcher);
    if !launcher.is_file() {
        return Err(RelayError::Failed(
            "Relay VZ launcher is not a regular file".into(),
        ));
    }
    if resources.state_directory.is_empty()
        || resources.launcher.contains('\0')
        || resources.state_directory.contains('\0')
    {
        return Err(RelayError::Failed(
            "Relay VZ runtime resources contain an invalid path".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn resolved_manifest(
    manifest: &GuestManifest,
    guest_directory: Option<&str>,
) -> Result<GuestManifest, RelayError> {
    let mut manifest = manifest.clone();
    manifest.kernel = resolve_artifact(&manifest.kernel, guest_directory)?;
    manifest.rootfs = resolve_artifact(&manifest.rootfs, guest_directory)?;
    manifest.initrd = manifest
        .initrd
        .as_ref()
        .map(|artifact| resolve_artifact(artifact, guest_directory))
        .transpose()?;
    Ok(manifest)
}

#[cfg(target_os = "macos")]
fn resolve_artifact(
    artifact: &GuestArtifact,
    guest_directory: Option<&str>,
) -> Result<GuestArtifact, RelayError> {
    let path = Path::new(&artifact.path);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        let root = guest_directory.ok_or_else(|| {
            RelayError::Failed("relative guest artifact requires guest_directory".into())
        })?;
        let root = fs::canonicalize(root).map_err(|error| {
            RelayError::Failed(format!("cannot resolve guest bundle directory: {error}"))
        })?;
        let candidate = fs::canonicalize(root.join(path)).map_err(|error| {
            RelayError::Failed(format!("cannot resolve guest artifact: {error}"))
        })?;
        if !candidate.starts_with(&root) {
            return Err(RelayError::Failed(
                "guest artifact escapes the guest bundle".into(),
            ));
        }
        candidate
    };
    let mut artifact = artifact.clone();
    artifact.path = resolved.display().to_string();
    Ok(artifact)
}

#[cfg(target_os = "macos")]
fn prepare_writable_disk(
    source: &Path,
    destination: &Path,
    minimum_bytes: u64,
) -> Result<(), RelayError> {
    if destination.exists() {
        let metadata = fs::metadata(destination).map_err(|error| {
            RelayError::Failed(format!("cannot inspect persisted VM disk: {error}"))
        })?;
        if !metadata.is_file() || metadata.len() < minimum_bytes {
            return Err(RelayError::Failed(
                "persisted VM disk is invalid or smaller than its immutable base".into(),
            ));
        }
        return make_owner_writable(destination);
    }

    unsafe extern "C" {
        fn clonefile(source: *const i8, destination: *const i8, flags: u32) -> i32;
    }

    let source_c = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| RelayError::Failed("guest disk path contains NUL".into()))?;
    let destination_c = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| RelayError::Failed("VM state path contains NUL".into()))?;
    if unsafe { clonefile(source_c.as_ptr(), destination_c.as_ptr(), 0) } != 0 {
        fs::copy(source, destination).map_err(|error| {
            RelayError::Failed(format!("cannot create writable VM disk: {error}"))
        })?;
    }
    make_owner_writable(destination)
}

#[cfg(target_os = "macos")]
fn make_owner_writable(destination: &Path) -> Result<(), RelayError> {
    let metadata = fs::metadata(destination)
        .map_err(|error| RelayError::Failed(format!("cannot stat writable VM disk: {error}")))?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | 0o600);
    fs::set_permissions(destination, permissions)
        .map_err(|error| RelayError::Failed(format!("cannot make VM disk writable: {error}")))
}

#[cfg(target_os = "macos")]
fn wait_for_guest_ready(
    child: &mut Child,
    log_path: &Path,
    timeout: Duration,
) -> Result<(), RelayError> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            RelayError::Failed(format!("cannot inspect Relay VZ process: {error}"))
        })? {
            return Err(RelayError::Failed(format!(
                "Relay VZ launcher exited before guest readiness: {status}\n{}",
                recent_log(log_path)
            )));
        }
        let log = fs::read_to_string(log_path).unwrap_or_default();
        if log.contains("WAWONA_RELAY_READY=1") {
            return Ok(());
        }
        if started.elapsed() >= timeout {
            return Err(RelayError::Failed(format!(
                "Relay VZ guest readiness timed out\n{}",
                recent_log(log_path)
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(target_os = "macos")]
fn recent_log(log_path: &Path) -> String {
    let bytes = fs::read(log_path).unwrap_or_default();
    let start = bytes.len().saturating_sub(16 * 1024);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

fn start_kvm(_spec: &RelaySpec, backend: RelayBackend) -> Result<RelayHandle, RelayError> {
    if !std::path::Path::new("/dev/kvm").exists() {
        return Err(RelayError::Failed(
            "/dev/kvm missing. Fail closed. No QEMU TCG".into(),
        ));
    }
    let _ = backend;
    Err(RelayError::Planned(
        "Relay KVM launch is not implemented yet; no VM was started",
    ))
}

pub fn stop(handle: &str) -> Result<(), RelayError> {
    #[cfg(target_os = "macos")]
    {
        let Some(sessions) = VZ_SESSIONS.get() else {
            return Err(RelayError::Failed("Relay VM handle is not running".into()));
        };
        let mut session = sessions
            .lock()
            .map_err(|_| RelayError::Failed("Relay VZ session registry is poisoned".into()))?
            .remove(handle)
            .ok_or_else(|| RelayError::Failed("Relay VM handle is not running".into()))?;
        terminate_child(&mut session.child)?;
        let _ = fs::remove_file(session.endpoint);
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = handle;
        Ok(())
    }
}

pub fn status(handle: &str) -> Result<RelayVmStatus, RelayError> {
    #[cfg(target_os = "macos")]
    {
        let sessions = VZ_SESSIONS
            .get()
            .ok_or_else(|| RelayError::Failed("Relay VM handle is not running".into()))?;
        let mut sessions = sessions
            .lock()
            .map_err(|_| RelayError::Failed("Relay VZ session registry is poisoned".into()))?;
        let session = sessions
            .get_mut(handle)
            .ok_or_else(|| RelayError::Failed("Relay VM handle is not running".into()))?;
        return session
            .child
            .try_wait()
            .map(|status| {
                if status.is_some() {
                    RelayVmStatus::Exited
                } else {
                    RelayVmStatus::Running
                }
            })
            .map_err(|error| RelayError::Failed(format!("cannot inspect Relay VM: {error}")));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = handle;
        Err(RelayError::Planned(
            "Relay VM status is not implemented on this backend",
        ))
    }
}

pub fn console_log(handle: &str) -> Result<Vec<u8>, RelayError> {
    #[cfg(target_os = "macos")]
    {
        let sessions = VZ_SESSIONS
            .get()
            .ok_or_else(|| RelayError::Failed("Relay VM handle is not running".into()))?;
        let sessions = sessions
            .lock()
            .map_err(|_| RelayError::Failed("Relay VZ session registry is poisoned".into()))?;
        let session = sessions
            .get(handle)
            .ok_or_else(|| RelayError::Failed("Relay VM handle is not running".into()))?;
        return fs::read(&session.log_path)
            .map_err(|error| RelayError::Failed(format!("cannot read Relay VM log: {error}")));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = handle;
        Err(RelayError::Planned(
            "Relay VM logs are not implemented on this backend",
        ))
    }
}

#[cfg(target_os = "macos")]
fn terminate_child(child: &mut Child) -> Result<(), RelayError> {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    const SIGTERM: i32 = 15;

    let _ = unsafe { kill(child.id() as i32, SIGTERM) };
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if child
            .try_wait()
            .map_err(|error| RelayError::Failed(format!("cannot stop Relay VM: {error}")))?
            .is_some()
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    child
        .kill()
        .map_err(|error| RelayError::Failed(format!("cannot terminate Relay VM: {error}")))?;
    child
        .wait()
        .map(|_| ())
        .map_err(|error| RelayError::Failed(format!("cannot reap Relay VM: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::{ArtifactClass, GuestArtifact, GuestManifest, RelayPlatform};

    #[test]
    fn macos_vz_fails_closed_without_runtime_resources() {
        let spec = RelaySpec {
            kind: RelayKind::Vm,
            platform: RelayPlatform::Macos,
            artifact: ArtifactClass::ModeA,
            machine_id: None,
            image: None,
            memory_mb: None,
            guest_page_size: None,
            guest: None,
            resources: None,
            ios_hv_host: None,
        };
        assert!(matches!(start(&spec), Err(RelayError::Failed(_))));
    }

    #[test]
    fn static_cpu_vm_without_guest_never_returns_proof_frame() {
        let spec = RelaySpec {
            kind: RelayKind::Vm,
            platform: RelayPlatform::Ios,
            artifact: ArtifactClass::ModeA,
            machine_id: None,
            image: None,
            memory_mb: None,
            guest_page_size: Some(4096),
            guest: None,
            resources: None,
            ios_hv_host: None,
        };
        let error = start(&spec).unwrap_err();
        assert!(error.to_string().contains("requires a guest manifest"));
    }

    #[test]
    fn ios_hv_start_is_planned_until_vcpu_exists() {
        let spec = RelaySpec {
            kind: RelayKind::Vm,
            platform: RelayPlatform::Ios,
            artifact: ArtifactClass::ModeB,
            machine_id: None,
            image: None,
            memory_mb: None,
            guest_page_size: None,
            guest: None,
            resources: None,
            ios_hv_host: Some(relay_core::IosHvHost::iphone_14_pro_16_3_1()),
        };
        assert!(matches!(start(&spec), Err(RelayError::Planned(_))));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_vz_process_lifecycle_waits_for_guest_readiness() {
        use sha2::{Digest, Sha256};
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "relay-vz-test-{}-{}",
            std::process::id(),
            NEXT_VZ_SESSION.fetch_add(1, Ordering::Relaxed)
        ));
        let guest_root = root.join("guest");
        let state_root = root.join("state");
        fs::create_dir_all(&guest_root).unwrap();
        let launcher = root.join("fake-vz");
        fs::write(
            &launcher,
            b"#!/bin/sh\necho 'WAWONA_RELAY_READY=1'\nwhile :; do sleep 1; done\n",
        )
        .unwrap();
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755)).unwrap();
        for name in ["Image", "initrd", "rootfs.img"] {
            fs::write(guest_root.join(name), name.as_bytes()).unwrap();
        }
        let artifact = |name: &str| {
            let bytes = fs::read(guest_root.join(name)).unwrap();
            GuestArtifact {
                path: name.into(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: bytes.len() as u64,
            }
        };
        let spec = RelaySpec {
            kind: RelayKind::Vm,
            platform: RelayPlatform::Macos,
            artifact: ArtifactClass::ModeA,
            machine_id: Some("test-vm".into()),
            image: None,
            memory_mb: Some(256),
            guest_page_size: Some(4096),
            guest: Some(GuestManifest {
                version: GuestManifest::VERSION,
                page_size: 4096,
                memory_bytes: 256 * 1024 * 1024,
                kernel: artifact("Image"),
                initrd: Some(artifact("initrd")),
                rootfs: artifact("rootfs.img"),
                command_line: "console=hvc0".into(),
                compatibility: relay_core::GuestCompatibility::default(),
                signature: None,
            }),
            resources: Some(RelayRuntimeResources {
                launcher: launcher.display().to_string(),
                state_directory: state_root.display().to_string(),
                guest_directory: Some(guest_root.display().to_string()),
                trusted_guest_keys: Default::default(),
                allow_unsigned_guest: true,
            }),
            ios_hv_host: None,
        };
        let handle = start(&spec).unwrap();
        assert_eq!(handle.backend, RelayBackend::Vz);
        assert!(handle.wayland_endpoint.is_some());
        assert_eq!(status(&handle.id).unwrap(), RelayVmStatus::Running);
        assert!(console_log(&handle.id)
            .unwrap()
            .windows(b"WAWONA_RELAY_READY=1".len())
            .any(|window| window == b"WAWONA_RELAY_READY=1"));
        let session_root = state_root.join("machines/test-vm");
        assert!(session_root.join("rootfs.img").is_file());
        assert!(session_root.join("console.log").is_file());
        stop(&handle.id).unwrap();
        OpenOptions::new()
            .append(true)
            .open(session_root.join("rootfs.img"))
            .unwrap()
            .write_all(b"persisted")
            .unwrap();
        let restarted = start(&spec).unwrap();
        assert!(fs::read(session_root.join("rootfs.img"))
            .unwrap()
            .ends_with(b"persisted"));
        assert_eq!(WAYPIPE_VSOCK_PORT, 1024);
        assert_eq!(OCI_SHARE_TAG, "oci-bundle");
        stop(&restarted.id).unwrap();
        assert!(stop(&handle.id).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

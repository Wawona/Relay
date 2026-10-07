//! Linux VM start/stop. Containers always sit on this backend.

mod aot;
pub use aot::ram_span_ok;
mod boot_trace;
mod bus;
mod cpu;
#[cfg(feature = "kernel-probe")]
pub use cpu::kernel_probe;
pub mod differential;
mod dtb;
mod exception;
mod float_arithmetic;
mod float_convert;
mod guest;
mod guest_session;
mod host_waypipe;
mod shm_import;
mod session_auth_host;
mod linux_boot;
mod mmu;
mod nixos_generations;
pub mod page_translate;
mod shutdown;
mod storage;
mod stream_bridge;
mod sysregs;
mod timer;
mod net_frame;
#[cfg(not(kani))]
mod net_host;
pub use bus::net_irq_iars;
#[cfg(not(kani))]
pub use net_host::{nat_counters, nat_io_counters};
#[cfg(not(kani))]
pub use virtio_net::rx_counters;
mod virtio_block;
mod virtio_console;
mod virtio_mmio;
mod virtio_net;
mod virtio_vsock;
mod vsock;
mod vsock_wire;

pub use guest::GuestMemory;
pub use guest_session::{
    frame_hash, verify_response, SessionChallenge, SessionKey, SessionResponse,
    CONTROL_VSOCK_PORT, PROTOCOL_VERSION,
};
pub use host_waypipe::HostWaypipeEntry;
pub use shm_import::ImportedFrame;
pub use session_auth_host::{AuthListener, AuthenticatedReady};
pub use nixos_generations::generations_json;
pub use page_translate::PageTranslate;
pub use stream_bridge::{BridgeProgress, StreamBridge};
pub use vsock::VsockConnection;
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub fn fuzz_vsock_packets(input: &[u8]) {
    vsock::fuzz_packets(input);
}

use relay_core::{
    resolve_backend, GuestArtifact, GuestManifest, RelayBackend, RelayError, RelayKind,
    RelayRuntimeResources, RelaySpec,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
};

#[cfg(target_os = "macos")]
use std::{
    fs::OpenOptions,
    os::unix::ffi::OsStrExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// Guest waypipe connects to host CID 2 on this port. The host accepts.
pub const WAYPIPE_VSOCK_PORT: u32 = 1024;

/// Virtiofs tag for OCI runtime bundles shared into the guest.
#[cfg(target_os = "macos")]
pub const OCI_SHARE_TAG: &str = "oci-bundle";

#[cfg(target_os = "macos")]
static NEXT_VZ_SESSION: AtomicU64 = AtomicU64::new(1);

static NEXT_STATIC_SESSION: AtomicU64 = AtomicU64::new(1);
static STATIC_SESSIONS: OnceLock<Mutex<BTreeMap<String, StaticSession>>> = OnceLock::new();

struct StaticSession {
    // Keep exclusive machine storage until its native worker also stops.
    _disk_guard: Option<std::fs::File>,
    stop: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
    console: Arc<Mutex<Vec<u8>>>,
    vsock_connections: Option<std::sync::mpsc::Receiver<VsockConnection>>,
    host_waypipe: Option<host_waypipe::Worker>,
    shm_importer: Option<shm_import::Importer>,
    imported_frame: Arc<Mutex<Option<ImportedFrame>>>,
    auth_listener: Option<AuthListener>,
    pc: Arc<AtomicU64>,
    physical_pc: Arc<AtomicU64>,
    instructions: Arc<AtomicU64>,
    x0: Arc<AtomicU64>,
    x1: Arc<AtomicU64>,
    x3: Arc<AtomicU64>,
    x30: Arc<AtomicU64>,
    lock_byte: Arc<AtomicU8>,
    lock_writer_pc: Arc<AtomicU64>,
    lock_writer_value: Arc<AtomicU64>,
    lock_writer_bytes: Arc<AtomicU8>,
    lock_prior_writer_pc: Arc<AtomicU64>,
    lock_prior_writer_value: Arc<AtomicU64>,
    lock_prior_writer_bytes: Arc<AtomicU8>,
    last_abort_pc: Arc<AtomicU64>,
    last_abort_insn: Arc<AtomicU64>,
    last_abort_prior_pc: Arc<AtomicU64>,
    last_abort_prior_insn: Arc<AtomicU64>,
    last_abort_address: Arc<AtomicU64>,
    last_abort_esr: Arc<AtomicU64>,
    last_abort_x0: Arc<AtomicU64>,
    last_abort_x1: Arc<AtomicU64>,
    last_abort_x2: Arc<AtomicU64>,
    last_abort_x3: Arc<AtomicU64>,
    last_abort_x30: Arc<AtomicU64>,
    last_abort_sp: Arc<AtomicU64>,
    abort_count: Arc<AtomicU64>,
    mmio_reads: Arc<AtomicU64>,
    mmio_writes: Arc<AtomicU64>,
    pl011_writes: Arc<AtomicU64>,
    last_mmio_read: Arc<AtomicU64>,
    last_mmio_write: Arc<AtomicU64>,
    gicc_iar_reads: Arc<AtomicU64>,
    gicc_spurious_reads: Arc<AtomicU64>,
    gicc_eoir_writes: Arc<AtomicU64>,
    last_irq_ack: Arc<AtomicU64>,
    last_irq_eoi: Arc<AtomicU64>,
    console_rx_notifications: Arc<AtomicU64>,
    console_rx_completions: Arc<AtomicU64>,
    console_tx_notifications: Arc<AtomicU64>,
    console_tx_completions: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(test)]
impl Default for StaticSession {
    fn default() -> Self {
        Self {
            _disk_guard: None,
            stop: Arc::new(AtomicBool::new(false)),
            state: Arc::new(AtomicU8::new(0)),
            console: Arc::new(Mutex::new(Vec::new())),
            vsock_connections: None,
            host_waypipe: None,
            shm_importer: None,
            imported_frame: Arc::new(Mutex::new(None)),
            auth_listener: None,
            pc: Arc::new(AtomicU64::new(0)),
            physical_pc: Arc::new(AtomicU64::new(0)),
            instructions: Arc::new(AtomicU64::new(0)),
            x0: Arc::new(AtomicU64::new(0)),
            x1: Arc::new(AtomicU64::new(0)),
            x3: Arc::new(AtomicU64::new(0)),
            x30: Arc::new(AtomicU64::new(0)),
            lock_byte: Arc::new(AtomicU8::new(0xff)),
            lock_writer_pc: Arc::new(AtomicU64::new(0)),
            lock_writer_value: Arc::new(AtomicU64::new(0)),
            lock_writer_bytes: Arc::new(AtomicU8::new(0)),
            lock_prior_writer_pc: Arc::new(AtomicU64::new(0)),
            lock_prior_writer_value: Arc::new(AtomicU64::new(0)),
            lock_prior_writer_bytes: Arc::new(AtomicU8::new(0)),
            last_abort_pc: Arc::new(AtomicU64::new(0)),
            last_abort_insn: Arc::new(AtomicU64::new(0)),
            last_abort_prior_pc: Arc::new(AtomicU64::new(0)),
            last_abort_prior_insn: Arc::new(AtomicU64::new(0)),
            last_abort_address: Arc::new(AtomicU64::new(0)),
            last_abort_esr: Arc::new(AtomicU64::new(0)),
            last_abort_x0: Arc::new(AtomicU64::new(0)),
            last_abort_x1: Arc::new(AtomicU64::new(0)),
            last_abort_x2: Arc::new(AtomicU64::new(0)),
            last_abort_x3: Arc::new(AtomicU64::new(0)),
            last_abort_x30: Arc::new(AtomicU64::new(0)),
            last_abort_sp: Arc::new(AtomicU64::new(0)),
            abort_count: Arc::new(AtomicU64::new(0)),
            mmio_reads: Arc::new(AtomicU64::new(0)),
            mmio_writes: Arc::new(AtomicU64::new(0)),
            pl011_writes: Arc::new(AtomicU64::new(0)),
            last_mmio_read: Arc::new(AtomicU64::new(0)),
            last_mmio_write: Arc::new(AtomicU64::new(0)),
            gicc_iar_reads: Arc::new(AtomicU64::new(0)),
            gicc_spurious_reads: Arc::new(AtomicU64::new(0)),
            gicc_eoir_writes: Arc::new(AtomicU64::new(0)),
            last_irq_ack: Arc::new(AtomicU64::new(0)),
            last_irq_eoi: Arc::new(AtomicU64::new(0)),
            console_rx_notifications: Arc::new(AtomicU64::new(0)),
            console_rx_completions: Arc::new(AtomicU64::new(0)),
            console_tx_notifications: Arc::new(AtomicU64::new(0)),
            console_tx_completions: Arc::new(AtomicU64::new(0)),
            thread: None,
        }
    }
}

#[cfg(target_os = "macos")]
static VZ_SESSIONS: OnceLock<Mutex<BTreeMap<String, VzSession>>> = OnceLock::new();

#[cfg(target_os = "macos")]
struct VzSession {
    _disk_guard: std::fs::File,
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
        RelayBackend::StaticCpu | RelayBackend::ModeBJit => start_ios(spec, backend),
        RelayBackend::IosHv => start_ios_hv(spec),
        RelayBackend::WasmPulley
        | RelayBackend::WasmCranelift
        | RelayBackend::WasmWasmerWebKit
        | RelayBackend::None => Err(RelayError::Failed("vm crate does not start wasm".into())),
    }
}

fn start_ios(spec: &RelaySpec, backend: RelayBackend) -> Result<RelayHandle, RelayError> {
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
    let mut manifest = resolved_manifest(manifest, resources.guest_directory.as_deref())?;
    manifest.memory_bytes = spec.vm_memory_bytes(manifest.memory_bytes)?;
    let (disk_gib, quota) = spec.vm_disk_limits()?;
    guest::validate_artifacts(&manifest)?;
    if spec.nixos_generation.is_some() && resources.state_directory.is_empty() {
        return Err(RelayError::Failed(
            "NixOS generation selection needs the machine disk".into(),
        ));
    }
    let (disk, disk_guard, trace_path) = if resources.state_directory.is_empty() {
        // Explicit development-only ephemeral sessions retain an in-memory overlay.
        (
            virtio_block::BlockDevice::from_file(Path::new(&manifest.rootfs.path), disk_gib)?,
            None,
            None,
        )
    } else {
        let machine = machine_state_key(spec.machine_id.as_deref())?;
        let directory = Path::new(&resources.state_directory)
            .join("machines")
            .join(machine);
        fs::create_dir_all(&directory).map_err(|e| RelayError::Failed(e.to_string()))?;
        let mut file = storage::open(
            Path::new(&manifest.rootfs.path),
            &directory.join("rootfs.img"),
            disk_gib,
            quota,
        )?;
        if let Some(generation) = spec.nixos_generation {
            nixos_generations::activate_generation(&mut file, generation)?;
        }
        let guard = file.try_clone().map_err(|error| {
            RelayError::Failed(format!("cannot retain VM disk ownership: {error}"))
        })?;
        (
            virtio_block::BlockDevice::persistent(file)?,
            Some(guard),
            Some(directory.join("console.log")),
        )
    };
    let (mut cpu, _) = linux_boot::create_cpu_with_disk(&manifest, Some(disk))?;
    let vsock_connections = cpu.listen_vsock(WAYPIPE_VSOCK_PORT)?;
    let auth_connections = cpu.listen_vsock(CONTROL_VSOCK_PORT)?;
    let mut boot_trace = trace_path
        .as_deref()
        .and_then(|path| boot_trace::BootTrace::open(path, manifest.memory_bytes, disk_gib));
    let id = format!(
        "static-{}",
        NEXT_STATIC_SESSION.fetch_add(1, Ordering::Relaxed)
    );
    let stop = Arc::new(AtomicBool::new(false));
    let state = Arc::new(AtomicU8::new(0));
    let console = Arc::new(Mutex::new(Vec::new()));
    let thread_stop = Arc::clone(&stop);
    let thread_state = Arc::clone(&state);
    let thread_console = Arc::clone(&console);
    let pc = Arc::new(AtomicU64::new(cpu.pc));
    let physical_pc = Arc::new(AtomicU64::new(cpu.debug_physical_pc()));
    let instructions = Arc::new(AtomicU64::new(0));
    let thread_pc = Arc::clone(&pc);
    let thread_physical_pc = Arc::clone(&physical_pc);
    let thread_instructions = Arc::clone(&instructions);
    let x0 = Arc::new(AtomicU64::new(0));
    let x1 = Arc::new(AtomicU64::new(0));
    let x3 = Arc::new(AtomicU64::new(0));
    let x30 = Arc::new(AtomicU64::new(0));
    let thread_x0 = Arc::clone(&x0);
    let thread_x1 = Arc::clone(&x1);
    let thread_x3 = Arc::clone(&x3);
    let thread_x30 = Arc::clone(&x30);
    let lock_byte = Arc::new(AtomicU8::new(0xff));
    let thread_lock_byte = Arc::clone(&lock_byte);
    let lock_writer_pc = Arc::new(AtomicU64::new(0));
    let lock_writer_value = Arc::new(AtomicU64::new(0));
    let lock_writer_bytes = Arc::new(AtomicU8::new(0));
    let thread_lock_writer_pc = Arc::clone(&lock_writer_pc);
    let thread_lock_writer_value = Arc::clone(&lock_writer_value);
    let thread_lock_writer_bytes = Arc::clone(&lock_writer_bytes);
    let lock_prior_writer_pc = Arc::new(AtomicU64::new(0));
    let lock_prior_writer_value = Arc::new(AtomicU64::new(0));
    let lock_prior_writer_bytes = Arc::new(AtomicU8::new(0));
    let thread_lock_prior_writer_pc = Arc::clone(&lock_prior_writer_pc);
    let thread_lock_prior_writer_value = Arc::clone(&lock_prior_writer_value);
    let thread_lock_prior_writer_bytes = Arc::clone(&lock_prior_writer_bytes);
    let last_abort_pc = Arc::new(AtomicU64::new(0));
    let last_abort_insn = Arc::new(AtomicU64::new(0));
    let last_abort_prior_pc = Arc::new(AtomicU64::new(0));
    let last_abort_prior_insn = Arc::new(AtomicU64::new(0));
    let last_abort_address = Arc::new(AtomicU64::new(0));
    let last_abort_esr = Arc::new(AtomicU64::new(0));
    let last_abort_x0 = Arc::new(AtomicU64::new(0));
    let last_abort_x1 = Arc::new(AtomicU64::new(0));
    let last_abort_x2 = Arc::new(AtomicU64::new(0));
    let last_abort_x3 = Arc::new(AtomicU64::new(0));
    let last_abort_x30 = Arc::new(AtomicU64::new(0));
    let last_abort_sp = Arc::new(AtomicU64::new(0));
    let abort_count = Arc::new(AtomicU64::new(0));
    let thread_last_abort_pc = Arc::clone(&last_abort_pc);
    let thread_last_abort_insn = Arc::clone(&last_abort_insn);
    let thread_last_abort_prior_pc = Arc::clone(&last_abort_prior_pc);
    let thread_last_abort_prior_insn = Arc::clone(&last_abort_prior_insn);
    let thread_last_abort_address = Arc::clone(&last_abort_address);
    let thread_last_abort_esr = Arc::clone(&last_abort_esr);
    let thread_last_abort_x0 = Arc::clone(&last_abort_x0);
    let thread_last_abort_x1 = Arc::clone(&last_abort_x1);
    let thread_last_abort_x2 = Arc::clone(&last_abort_x2);
    let thread_last_abort_x3 = Arc::clone(&last_abort_x3);
    let thread_last_abort_x30 = Arc::clone(&last_abort_x30);
    let thread_last_abort_sp = Arc::clone(&last_abort_sp);
    let thread_abort_count = Arc::clone(&abort_count);
    let mmio_reads = Arc::new(AtomicU64::new(0));
    let mmio_writes = Arc::new(AtomicU64::new(0));
    let pl011_writes = Arc::new(AtomicU64::new(0));
    let last_mmio_read = Arc::new(AtomicU64::new(0));
    let last_mmio_write = Arc::new(AtomicU64::new(0));
    let thread_mmio_reads = Arc::clone(&mmio_reads);
    let thread_mmio_writes = Arc::clone(&mmio_writes);
    let thread_pl011_writes = Arc::clone(&pl011_writes);
    let thread_last_mmio_read = Arc::clone(&last_mmio_read);
    let thread_last_mmio_write = Arc::clone(&last_mmio_write);
    let gicc_iar_reads = Arc::new(AtomicU64::new(0));
    let gicc_spurious_reads = Arc::new(AtomicU64::new(0));
    let gicc_eoir_writes = Arc::new(AtomicU64::new(0));
    let last_irq_ack = Arc::new(AtomicU64::new(1023));
    let last_irq_eoi = Arc::new(AtomicU64::new(1023));
    let thread_gicc_iar_reads = Arc::clone(&gicc_iar_reads);
    let thread_gicc_spurious_reads = Arc::clone(&gicc_spurious_reads);
    let thread_gicc_eoir_writes = Arc::clone(&gicc_eoir_writes);
    let thread_last_irq_ack = Arc::clone(&last_irq_ack);
    let thread_last_irq_eoi = Arc::clone(&last_irq_eoi);
    let console_rx_notifications = Arc::new(AtomicU64::new(0));
    let console_rx_completions = Arc::new(AtomicU64::new(0));
    let console_tx_notifications = Arc::new(AtomicU64::new(0));
    let console_tx_completions = Arc::new(AtomicU64::new(0));
    let thread_console_rx_notifications = Arc::clone(&console_rx_notifications);
    let thread_console_rx_completions = Arc::clone(&console_rx_completions);
    let thread_console_tx_notifications = Arc::clone(&console_tx_notifications);
    let thread_console_tx_completions = Arc::clone(&console_tx_completions);
    let thread = thread::Builder::new()
        .name(id.clone())
        .spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                // A static interpreter must return to its lifecycle boundary
                // frequently enough for Stop to be prompt.  This is not a
                // guest timeslice policy: it only bounds host cancellation
                // latency while the guest still owns a single vCPU.
                // 4K instructions keeps cancellation sub-millisecond on the
                // static interpreter while avoiding a mutex round-trip every
                // handful of guest instructions during Linux boot.
                if let Err(error) = cpu.run_slice(4_096) {
                    if let Some(trace) = boot_trace.as_mut() {
                        trace.finish(
                            cpu.console(),
                            cpu.pc,
                            thread_instructions.load(Ordering::Relaxed),
                            &error.to_string(),
                        );
                    }
                    if let Ok(mut output) = thread_console.lock() {
                        output.clear();
                        output.extend_from_slice(cpu.console());
                        output
                            .extend_from_slice(format!("\nRelay StaticCpu: {error}\n").as_bytes());
                    }
                    thread_state.store(2, Ordering::Release);
                    return;
                }
                thread_pc.store(cpu.pc, Ordering::Release);
                thread_physical_pc.store(cpu.debug_physical_pc(), Ordering::Release);
                let executed = thread_instructions.fetch_add(4_096, Ordering::Relaxed) + 4_096;
                if executed & ((1 << 20) - 1) == 0 {
                    if let Some(trace) = boot_trace.as_mut() {
                        trace.observe(cpu.console(), cpu.pc, executed);
                    }
                }
                let (
                    current_x0,
                    current_x1,
                    current_x3,
                    current_x30,
                    current_lock_byte,
                    writer_pc,
                    writer_value,
                    writer_bytes,
                    prior_writer_pc,
                    prior_writer_value,
                    prior_writer_bytes,
                    current_last_abort_pc,
                    current_last_abort_insn,
                    current_last_abort_prior_pc,
                    current_last_abort_prior_insn,
                    current_last_abort_address,
                    current_last_abort_esr,
                    current_last_abort_x0,
                    current_last_abort_x1,
                    current_last_abort_x2,
                    current_last_abort_x3,
                    current_last_abort_x30,
                    current_last_abort_sp,
                    current_abort_count,
                ) = cpu.debug_registers();
                thread_x0.store(current_x0, Ordering::Release);
                thread_x1.store(current_x1, Ordering::Release);
                thread_x3.store(current_x3, Ordering::Release);
                thread_x30.store(current_x30, Ordering::Release);
                thread_lock_byte.store(current_lock_byte, Ordering::Release);
                thread_lock_writer_pc.store(writer_pc, Ordering::Release);
                thread_lock_writer_value.store(writer_value, Ordering::Release);
                thread_lock_writer_bytes.store(writer_bytes, Ordering::Release);
                thread_lock_prior_writer_pc.store(prior_writer_pc, Ordering::Release);
                thread_lock_prior_writer_value.store(prior_writer_value, Ordering::Release);
                thread_lock_prior_writer_bytes.store(prior_writer_bytes, Ordering::Release);
                thread_last_abort_pc.store(current_last_abort_pc, Ordering::Release);
                thread_last_abort_insn.store(u64::from(current_last_abort_insn), Ordering::Release);
                thread_last_abort_prior_pc.store(current_last_abort_prior_pc, Ordering::Release);
                thread_last_abort_prior_insn
                    .store(u64::from(current_last_abort_prior_insn), Ordering::Release);
                thread_last_abort_address.store(current_last_abort_address, Ordering::Release);
                thread_last_abort_esr.store(current_last_abort_esr, Ordering::Release);
                thread_last_abort_x0.store(current_last_abort_x0, Ordering::Release);
                thread_last_abort_x1.store(current_last_abort_x1, Ordering::Release);
                thread_last_abort_x2.store(current_last_abort_x2, Ordering::Release);
                thread_last_abort_x3.store(current_last_abort_x3, Ordering::Release);
                thread_last_abort_x30.store(current_last_abort_x30, Ordering::Release);
                thread_last_abort_sp.store(current_last_abort_sp, Ordering::Release);
                thread_abort_count.store(current_abort_count, Ordering::Release);
                let (
                    current_mmio_reads,
                    current_mmio_writes,
                    current_pl011_writes,
                    current_last_mmio_read,
                    current_last_mmio_write,
                    current_gicc_iar_reads,
                    current_gicc_spurious_reads,
                    current_gicc_eoir_writes,
                    current_last_irq_ack,
                    current_last_irq_eoi,
                    current_console_rx_notifications,
                    current_console_rx_completions,
                    current_console_tx_notifications,
                    current_console_tx_completions,
                ) = cpu.debug_device_activity();
                thread_mmio_reads.store(current_mmio_reads, Ordering::Release);
                thread_mmio_writes.store(current_mmio_writes, Ordering::Release);
                thread_pl011_writes.store(current_pl011_writes, Ordering::Release);
                thread_last_mmio_read.store(current_last_mmio_read, Ordering::Release);
                thread_last_mmio_write.store(current_last_mmio_write, Ordering::Release);
                thread_gicc_iar_reads.store(current_gicc_iar_reads, Ordering::Release);
                thread_gicc_spurious_reads.store(current_gicc_spurious_reads, Ordering::Release);
                thread_gicc_eoir_writes.store(current_gicc_eoir_writes, Ordering::Release);
                thread_last_irq_ack.store(current_last_irq_ack, Ordering::Release);
                thread_last_irq_eoi.store(current_last_irq_eoi, Ordering::Release);
                thread_console_rx_notifications
                    .store(current_console_rx_notifications, Ordering::Release);
                thread_console_rx_completions
                    .store(current_console_rx_completions, Ordering::Release);
                thread_console_tx_notifications
                    .store(current_console_tx_notifications, Ordering::Release);
                thread_console_tx_completions
                    .store(current_console_tx_completions, Ordering::Release);
                if let Ok(mut output) = thread_console.lock() {
                    // The bus log is append-only. Copy only new bytes rather
                    // than the entire boot transcript every 4096 instructions.
                    let copied = output.len();
                    output.extend_from_slice(&cpu.console()[copied..]);
                }
            }
            if let Some(trace) = boot_trace.as_mut() {
                trace.finish(
                    cpu.console(),
                    cpu.pc,
                    thread_instructions.load(Ordering::Relaxed),
                    "stopped",
                );
            }
            thread_state.store(1, Ordering::Release);
        })
        .map_err(|error| RelayError::Failed(format!("cannot start StaticCpu thread: {error}")))?;
    let mut seed = [0u8; 32];
    fill_os_random(&mut seed)?;
    let mut nonce = [0u8; 32];
    fill_os_random(&mut nonce)?;
    let machine_id = spec
        .machine_id
        .clone()
        .unwrap_or_else(|| id.clone());
    let key = SessionKey::derive(&seed, &manifest.kernel.sha256, &manifest.rootfs.sha256);
    let challenge = SessionChallenge::new(
        machine_id,
        id.clone(),
        manifest.kernel.sha256.clone(),
        manifest.rootfs.sha256.clone(),
        nonce,
    );
    let auth_listener = AuthListener::serve_connection(
        auth_connections,
        key,
        challenge,
        Arc::clone(&stop),
    );
    STATIC_SESSIONS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?
        .insert(
            id.clone(),
            StaticSession {
                _disk_guard: disk_guard,
                stop,
                state,
                console,
                vsock_connections: Some(vsock_connections),
                host_waypipe: None,
                shm_importer: None,
                imported_frame: Arc::new(Mutex::new(None)),
                auth_listener: Some(auth_listener),
                pc,
                physical_pc,
                instructions,
                x0,
                x1,
                x3,
                x30,
                lock_byte,
                lock_writer_pc,
                lock_writer_value,
                lock_writer_bytes,
                lock_prior_writer_pc,
                lock_prior_writer_value,
                lock_prior_writer_bytes,
                last_abort_pc,
                last_abort_insn,
                last_abort_prior_pc,
                last_abort_prior_insn,
                last_abort_address,
                last_abort_esr,
                last_abort_x0,
                last_abort_x1,
                last_abort_x2,
                last_abort_x3,
                last_abort_x30,
                last_abort_sp,
                abort_count,
                mmio_reads,
                mmio_writes,
                pl011_writes,
                last_mmio_read,
                last_mmio_write,
                gicc_iar_reads,
                gicc_spurious_reads,
                gicc_eoir_writes,
                last_irq_ack,
                last_irq_eoi,
                console_rx_notifications,
                console_rx_completions,
                console_tx_notifications,
                console_tx_completions,
                thread: Some(thread),
            },
        );
    Ok(RelayHandle {
        id,
        backend,
        kind: spec.kind,
        wayland_endpoint: None,
        frame: None,
        frame_width: 0,
        frame_height: 0,
    })
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
    let mut manifest = resolved_manifest(source_manifest, resources.guest_directory.as_deref())?;
    manifest.memory_bytes = spec.vm_memory_bytes(manifest.memory_bytes)?;
    let (disk_gib, quota) = spec.vm_disk_limits()?;
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
    let mut disk_guard = storage::open(Path::new(&manifest.rootfs.path), &disk, disk_gib, quota)?;
    if let Some(generation) = spec.nixos_generation {
        nixos_generations::activate_generation(&mut disk_guard, generation)?;
    }
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
                _disk_guard: disk_guard,
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

fn machine_state_key(machine_id: Option<&str>) -> Result<String, RelayError> {
    let Some(machine_id) = machine_id else {
        return Ok(format!(
            "ephemeral-{}-{}",
            std::process::id(),
            NEXT_STATIC_SESSION.fetch_add(1, Ordering::Relaxed)
        ));
    };
    if matches!(machine_id, "." | "..")
        || machine_id.is_empty()
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
    if let Some(sessions) = STATIC_SESSIONS.get() {
        let mut sessions = sessions
            .lock()
            .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
        if let Some(session) = sessions.get_mut(handle) {
            session.stop.store(true, Ordering::Release);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let result = (|| {
                // Drop the device's sockets before waiting for native waypipe's
                // blocking protocol read. The peer closure wakes its handler.
                match shutdown::join_until(&mut session.thread, deadline, "Relay StaticCpu") {
                    Ok(true) => {}
                    Ok(false) => {
                        return Err(RelayError::Failed(
                            "StaticCpu stop pending; session retained for retry".into(),
                        ))
                    }
                    Err(error) => {
                        session.state.store(2, Ordering::Release);
                        return Err(error);
                    }
                }
                if let Some(worker) = &mut session.host_waypipe {
                    if !worker.join_until(deadline)? {
                        return Err(RelayError::Failed(
                            "host waypipe stop pending; session retained for retry".into(),
                        ));
                    }
                }
                if let Some(importer) = &mut session.shm_importer {
                    if !importer.join_until(deadline)? {
                        return Err(RelayError::Failed(
                            "shm import stop pending; session retained for retry".into(),
                        ));
                    }
                }
                Ok(())
            })();
            // Keep the session visible and disk exclusively owned throughout
            // Stop, including pending and panic cleanup. Concurrent Stop cannot
            // mistake a temporarily removed handle for completed shutdown.
            if result.is_ok() {
                sessions.remove(handle);
            }
            return result;
        }
    }
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
    if let Some(sessions) = STATIC_SESSIONS.get() {
        if let Some(session) = sessions
            .lock()
            .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?
            .get(handle)
        {
            return Ok(if session.state.load(Ordering::Acquire) == 0 {
                RelayVmStatus::Running
            } else {
                RelayVmStatus::Exited
            });
        }
    }
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

/// Take a real guest-initiated stream from the StaticCpu waypipe listener.
/// This does not authenticate the guest or prove an imported frame.
pub fn take_vsock_connection(handle: &str) -> Result<Option<VsockConnection>, RelayError> {
    let sessions = STATIC_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
    let session = sessions
        .get(handle)
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu session not found".into()))?;
    let connections = session
        .vsock_connections
        .as_ref()
        .ok_or_else(|| RelayError::Failed("Relay vsock is owned by native host waypipe".into()))?;
    match connections.try_recv() {
        Ok(connection) => Ok(Some(connection)),
        Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            Err(RelayError::Failed("Relay vsock transport stopped".into()))
        }
    }
}

/// Attach the real native client to guest-initiated vsock streams. A reconnect
/// gets a fresh synchronous client invocation after the previous one returns.
/// No console marker, channel activity or return code publishes readiness.
///
/// # Safety
/// Entry must obey HostWaypipeEntry's borrowed descriptor/termination contract
/// and remain callable until stop succeeds. Native entry points are static.
pub unsafe fn start_host_waypipe(handle: &str, entry: HostWaypipeEntry) -> Result<(), RelayError> {
    let sessions = STATIC_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
    let session = sessions
        .get_mut(handle)
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu session not found".into()))?;
    if session.stop.load(Ordering::Acquire) || session.state.load(Ordering::Acquire) != 0 {
        return Err(RelayError::Failed(
            "Relay StaticCpu session is stopping or exited".into(),
        ));
    }
    let connections = session
        .vsock_connections
        .take()
        .ok_or_else(|| RelayError::Failed("Relay host waypipe is already attached".into()))?;
    // SAFETY: the API caller guarantees a static contracted native entry.
    let worker = unsafe {
        host_waypipe::Worker::start(
            connections,
            Arc::clone(&session.stop),
            Arc::clone(&session.state),
            entry,
        )
    };
    match worker {
        Ok(worker) => {
            session.host_waypipe = Some(worker);
            Ok(())
        }
        Err(error) => {
            session.stop.store(true, Ordering::Release);
            Err(error)
        }
    }
}

/// Last native client exit, if a stream has completed. Not a readiness result.
pub fn host_waypipe_last_exit(handle: &str) -> Result<Option<i32>, RelayError> {
    let sessions = STATIC_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
    let session = sessions
        .get(handle)
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu session not found".into()))?;
    Ok(session
        .host_waypipe
        .as_ref()
        .and_then(host_waypipe::Worker::last_exit))
}

/// Attach the SHM importer to guest waypipe vsock 1024. Native `start_host_waypipe`
/// cannot run on the same session: both consume that listener.
pub fn start_shm_importer(handle: &str) -> Result<(), RelayError> {
    let sessions = STATIC_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
    let session = sessions
        .get_mut(handle)
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu session not found".into()))?;
    if session.stop.load(Ordering::Acquire) || session.state.load(Ordering::Acquire) != 0 {
        return Err(RelayError::Failed(
            "Relay StaticCpu session is stopping or exited".into(),
        ));
    }
    let connections = session
        .vsock_connections
        .take()
        .ok_or_else(|| RelayError::Failed("Relay host waypipe is already attached".into()))?;
    let frame = Arc::clone(&session.imported_frame);
    let importer = shm_import::Importer::start(
        connections,
        Arc::clone(&session.stop),
        Arc::clone(&session.state),
        frame,
    )?;
    session.shm_importer = Some(importer);
    Ok(())
}

/// Guest SHM pixels imported from waypipe BUFFER_FILL + wl_surface.commit.
pub fn take_imported_frame(handle: &str) -> Result<Option<ImportedFrame>, RelayError> {
    let sessions = STATIC_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
    let session = sessions
        .get(handle)
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu session not found".into()))?;
    session
        .imported_frame
        .lock()
        .map(|guard| guard.clone())
        .map_err(|_| RelayError::Failed("Relay imported frame lock is poisoned".into()))
}

/// Authenticated guest READY on vsock 1025, if the HMAC exchange completed.
pub fn take_authenticated_ready(handle: &str) -> Result<Option<AuthenticatedReady>, RelayError> {
    let sessions = STATIC_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
    let session = sessions
        .get(handle)
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu session not found".into()))?;
    Ok(session
        .auth_listener
        .as_ref()
        .and_then(AuthListener::peek_ready))
}

fn fill_os_random(out: &mut [u8]) -> Result<(), RelayError> {
    let mut file = fs::File::open("/dev/urandom")
        .map_err(|e| RelayError::Failed(format!("cannot open /dev/urandom: {e}")))?;
    use std::io::Read;
    file.read_exact(out)
        .map_err(|e| RelayError::Failed(format!("cannot read /dev/urandom: {e}")))?;
    Ok(())
}

pub fn console_log(handle: &str) -> Result<Vec<u8>, RelayError> {
    if let Some(sessions) = STATIC_SESSIONS.get() {
        if let Some(session) = sessions
            .lock()
            .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?
            .get(handle)
        {
            return session
                .console
                .lock()
                .map(|output| output.clone())
                .map_err(|_| RelayError::Failed("Relay StaticCpu console is poisoned".into()));
        }
    }
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

/// Observable, non-mutating boot progress for the real static-CPU probe.
pub fn static_cpu_progress(
    handle: &str,
) -> Result<
    (
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u8,
        u64,
        u64,
        u8,
        u64,
        u64,
        u8,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ),
    RelayError,
> {
    let sessions = STATIC_SESSIONS
        .get()
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu handle is not running".into()))?;
    let sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
    let session = sessions
        .get(handle)
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu handle is not running".into()))?;
    Ok((
        session.pc.load(Ordering::Acquire),
        session.physical_pc.load(Ordering::Acquire),
        session.instructions.load(Ordering::Relaxed),
        session.x0.load(Ordering::Acquire),
        session.x1.load(Ordering::Acquire),
        session.x3.load(Ordering::Acquire),
        session.x30.load(Ordering::Acquire),
        session.lock_byte.load(Ordering::Acquire),
        session.lock_writer_pc.load(Ordering::Acquire),
        session.lock_writer_value.load(Ordering::Acquire),
        session.lock_writer_bytes.load(Ordering::Acquire),
        session.lock_prior_writer_pc.load(Ordering::Acquire),
        session.lock_prior_writer_value.load(Ordering::Acquire),
        session.lock_prior_writer_bytes.load(Ordering::Acquire),
        session.last_abort_pc.load(Ordering::Acquire),
        session.last_abort_insn.load(Ordering::Acquire),
        session.last_abort_prior_pc.load(Ordering::Acquire),
        session.last_abort_prior_insn.load(Ordering::Acquire),
        session.last_abort_address.load(Ordering::Acquire),
        session.last_abort_esr.load(Ordering::Acquire),
        session.last_abort_x0.load(Ordering::Acquire),
        session.last_abort_x1.load(Ordering::Acquire),
        session.last_abort_x2.load(Ordering::Acquire),
        session.last_abort_x3.load(Ordering::Acquire),
        session.last_abort_x30.load(Ordering::Acquire),
        session.last_abort_sp.load(Ordering::Acquire),
        session.abort_count.load(Ordering::Acquire),
    ))
}

/// MMIO activity observed by a running static-CPU guest.
pub fn static_device_progress(
    handle: &str,
) -> Result<
    (
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ),
    RelayError,
> {
    let sessions = STATIC_SESSIONS
        .get()
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu handle is not running".into()))?;
    let sessions = sessions
        .lock()
        .map_err(|_| RelayError::Failed("Relay StaticCpu registry is poisoned".into()))?;
    let session = sessions
        .get(handle)
        .ok_or_else(|| RelayError::Failed("Relay StaticCpu handle is not running".into()))?;
    Ok((
        session.mmio_reads.load(Ordering::Acquire),
        session.mmio_writes.load(Ordering::Acquire),
        session.pl011_writes.load(Ordering::Acquire),
        session.last_mmio_read.load(Ordering::Acquire),
        session.last_mmio_write.load(Ordering::Acquire),
        session.gicc_iar_reads.load(Ordering::Acquire),
        session.gicc_spurious_reads.load(Ordering::Acquire),
        session.gicc_eoir_writes.load(Ordering::Acquire),
        session.last_irq_ack.load(Ordering::Acquire),
        session.last_irq_eoi.load(Ordering::Acquire),
        session.console_rx_notifications.load(Ordering::Acquire),
        session.console_rx_completions.load(Ordering::Acquire),
        session.console_tx_notifications.load(Ordering::Acquire),
        session.console_tx_completions.load(Ordering::Acquire),
    ))
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

    #[cfg(not(miri))]
    #[test]
    fn pending_cpu_stop_retains_registry_and_exclusive_disk_until_retry() {
        let id = format!(
            "shutdown-test-{}",
            NEXT_STATIC_SESSION.fetch_add(1, Ordering::Relaxed)
        );
        let directory = std::env::temp_dir().join(format!("relay-{}-{}", id, std::process::id()));
        fs::create_dir(&directory).unwrap();
        let base = directory.join("base");
        let disk = directory.join("rootfs.img");
        fs::write(&base, b"persisted guest data").unwrap();
        let guard = storage::open(&base, &disk, 4, 64).unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let session = StaticSession {
            _disk_guard: Some(guard),
            thread: Some(thread::spawn(move || wait.recv().unwrap())),
            ..Default::default()
        };
        let cancel = Arc::clone(&session.stop);
        STATIC_SESSIONS
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .unwrap()
            .insert(id.clone(), session);
        assert!(stop(&id)
            .unwrap_err()
            .to_string()
            .contains("StaticCpu stop pending"));
        assert!(cancel.load(Ordering::Acquire));
        assert_eq!(status(&id).unwrap(), RelayVmStatus::Running);
        assert!(storage::open(&base, &disk, 5, 64).is_err());
        assert_eq!(fs::metadata(&disk).unwrap().len(), 4 << 30);
        release.send(()).unwrap();
        stop(&id).unwrap();
        assert!(!STATIC_SESSIONS
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .contains_key(&id));
        let reopened = storage::open(&base, &disk, 5, 64).unwrap();
        use std::io::Read;
        let mut actual = [0; 20];
        (&reopened).read_exact(&mut actual).unwrap();
        assert_eq!(&actual, b"persisted guest data");
        assert_eq!(reopened.metadata().unwrap().len(), 5 << 30);
        drop(reopened);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn machine_disk_namespace_rejects_parent_and_empty_keys() {
        for id in ["", ".", "..", "../neighbor", "a/b"] {
            assert!(machine_state_key(Some(id)).is_err());
        }
        assert_eq!(machine_state_key(Some("ios-vm.1")).unwrap(), "ios-vm.1");
        assert_ne!(
            machine_state_key(None).unwrap(),
            machine_state_key(None).unwrap()
        );
    }

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
            disk_gib: None,
            max_disk_gib: None,
            guest_page_size: None,
            guest: None,
            resources: None,
            ios_hv_host: None,
            nixos_generation: None,
            apple_os_major: None,
            wasmer_webkit_linked: false,
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
            disk_gib: None,
            max_disk_gib: None,
            guest_page_size: Some(4096),
            guest: None,
            resources: None,
            ios_hv_host: None,
            nixos_generation: None,
            apple_os_major: None,
            wasmer_webkit_linked: false,
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
            disk_gib: None,
            max_disk_gib: None,
            guest_page_size: None,
            guest: None,
            resources: None,
            ios_hv_host: Some(relay_core::IosHvHost::iphone_14_pro_16_3_1()),
            nixos_generation: None,
            apple_os_major: None,
            wasmer_webkit_linked: false,
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
            "rv-{}-{}",
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
            disk_gib: None,
            max_disk_gib: None,
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
            nixos_generation: None,
            apple_os_major: None,
            wasmer_webkit_linked: false,
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
        use std::os::unix::fs::FileExt;
        let disk = OpenOptions::new()
            .write(true)
            .read(true)
            .open(session_root.join("rootfs.img"))
            .unwrap();
        disk.write_all_at(b"persisted", 512).unwrap();
        let restarted = start(&spec).unwrap();
        let mut saved = [0; 9];
        disk.read_exact_at(&mut saved, 512).unwrap();
        assert_eq!(&saved, b"persisted");
        assert_eq!(WAYPIPE_VSOCK_PORT, 1024);
        assert_eq!(OCI_SHARE_TAG, "oci-bundle");
        stop(&restarted.id).unwrap();
        assert!(stop(&handle.id).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

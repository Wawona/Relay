use relay_core::{
    ArtifactClass, GuestArtifact, GuestManifest, RelayKind, RelayPlatform, RelayRuntimeResources,
    RelaySpec,
};
use std::{
    env, fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let guest_directory = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: static_smoke GUEST_DIRECTORY [SECONDS]")?;
    let seconds = arguments
        .next()
        .map(|value| value.to_string_lossy().parse::<u64>())
        .transpose()?
        .unwrap_or(5);
    let vsock_probe = match arguments.next() {
        None => false,
        Some(value) if value == "--vsock-probe" => true,
        Some(_) => {
            return Err("usage: static_smoke GUEST_DIRECTORY [SECONDS] [--vsock-probe]".into())
        }
    };
    let mut authenticated = None;
    if arguments.next().is_some() {
        return Err("usage: static_smoke GUEST_DIRECTORY [SECONDS] [--vsock-probe]".into());
    }

    let mut manifest: GuestManifest =
        serde_json::from_slice(&fs::read(guest_directory.join("manifest.json"))?)?;
    // Keep the independent early UART visible while proving the hvc0 queue
    // handoff. This exposes a stalled initcall even when Linux has selected
    // hvc0 but has not yet submitted its first transmit descriptor.
    if !manifest
        .command_line
        .split_whitespace()
        .any(|arg| arg == "keep_bootcon")
    {
        manifest.command_line.push_str(" keep_bootcon");
    }
    resolve(&guest_directory, &mut manifest.kernel);
    if let Some(initrd) = &mut manifest.initrd {
        resolve(&guest_directory, initrd);
    }
    resolve(&guest_directory, &mut manifest.rootfs);

    let spec = RelaySpec {
        kind: RelayKind::Vm,
        platform: RelayPlatform::Ios,
        artifact: ArtifactClass::ModeA,
        machine_id: Some(
            env::var("WAWONA_SMOKE_MACHINE_ID").unwrap_or_else(|_| "static-smoke".into()),
        ),
        image: None,
        memory_mb: Some((manifest.memory_bytes / (1024 * 1024)) as u32),
        disk_gib: env::var("WAWONA_SMOKE_DISK_GIB")
            .ok()
            .and_then(|value| value.parse().ok()),
        max_disk_gib: None,
        guest_page_size: Some(manifest.page_size),
        guest: Some(manifest),
        resources: Some(RelayRuntimeResources {
            launcher: String::new(),
            state_directory: env::var("WAWONA_SMOKE_STATE_DIR").unwrap_or_default(),
            guest_directory: Some(guest_directory.display().to_string()),
            trusted_guest_keys: Default::default(),
            allow_unsigned_guest: true,
        }),
        ios_hv_host: None,
        nixos_generation: None,
        apple_os_major: None,
        wasmer_webkit_linked: false,
    };
    let handle = relay_vm::start(&spec)?;
    println!("Relay static guest started: {}", handle.id);
    if !vsock_probe {
        relay_vm::start_shm_importer(&handle.id)?;
    }
    let mut imported = None;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    // Diagnostic only: observe real guest bytes without simulating waypipe or
    // publishing readiness. Retain at most the device's 16 bounded streams.
    let mut probe_streams = Vec::new();
    let mut emitted_console = 0;
    let mut next_console_poll = Instant::now();
    let mut next_progress_poll = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline
        && relay_vm::status(&handle.id)? == relay_vm::RelayVmStatus::Running
    {
        if vsock_probe {
            if let Some(connection) = relay_vm::take_vsock_connection(&handle.id)? {
                eprintln!(
                    "Relay vsock transport only: guest CID=3 port={} -> host CID=2 port={}",
                    connection.guest_port, connection.host_port
                );
                connection.stream.set_nonblocking(true)?;
                if probe_streams.len() < 16 {
                    probe_streams.push((connection.stream, false));
                }
            }
            probe_streams.retain_mut(|(stream, observed)| {
                use std::io::Read;
                let mut bytes = [0; 4096];
                match stream.read(&mut bytes) {
                    Ok(0) => false,
                    Ok(n) => {
                        if !*observed {
                            eprintln!("Relay vsock real guest payload: {} bytes, prefix={:02x?}; no authentication or frame evidence", n, &bytes[..n.min(64)]);
                            *observed = true;
                        }
                        true
                    }
                    Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => true,
                    Err(_) => false,
                }
            });
        }
        if Instant::now() >= next_console_poll {
            let console = relay_vm::console_log(&handle.id)?;
            if console.len() > emitted_console {
                eprint!("{}", String::from_utf8_lossy(&console[emitted_console..]));
                emitted_console = console.len();
            }
            if console_failure(&console).is_some()
                || console
                    .windows(b"wawona-fastfetch: end".len())
                    .any(|window| window == b"wawona-fastfetch: end")
            {
                break;
            }
            next_console_poll = Instant::now() + Duration::from_millis(250);
        }
        if Instant::now() >= next_progress_poll {
            let (pc, physical_pc, instructions, ..) = relay_vm::static_cpu_progress(&handle.id)?;
            let (
                reads,
                writes,
                _,
                last_read,
                last_write,
                iar,
                spurious,
                eoir,
                irq,
                eoi,
                rx_notify,
                rx_complete,
                tx_notify,
                tx_complete,
            ) = relay_vm::static_device_progress(&handle.id)?;
            let (syn, conn_ok, conn_fail, synack_ok, synack_drop, rst) =
                relay_vm::nat_counters();
            let (host_tx, host_rx) = relay_vm::nat_io_counters();
            let (rx_del, rx_nobuf, rx_small) = relay_vm::rx_counters();
            let iar67 = relay_vm::net_irq_iars();
            eprintln!(
                "Relay static heartbeat: instructions={instructions} pc={pc:#x}/{physical_pc:#x} mmio={reads}/{writes} last={last_read:#x}/{last_write:#x} gicc={iar}/{spurious}/{eoir} irq={irq}/{eoi} iar67={iar67} console-rx={rx_notify}/{rx_complete} console-tx={tx_notify}/{tx_complete} nat=syn:{syn}/ok:{conn_ok}/fail:{conn_fail}/synack:{synack_ok}/drop:{synack_drop}/rst:{rst}/htx:{host_tx}/hrx:{host_rx} rx=del:{rx_del}/nobuf:{rx_nobuf}/small:{rx_small}"
            );
            next_progress_poll = Instant::now() + Duration::from_secs(30);
        }
        if authenticated.is_none() {
            if let Some(ready) = relay_vm::take_authenticated_ready(&handle.id)? {
                eprintln!(
                    "Relay authenticated session: machine={} session={} unit={}",
                    ready.machine_id, ready.session_id, ready.unit_state
                );
                authenticated = Some(ready);
            }
        }
        if imported.is_none() {
            if let Some(frame) = relay_vm::take_imported_frame(&handle.id)? {
                eprintln!(
                    "Relay imported SHM frame: {}x{} sha256={}",
                    frame.width,
                    frame.height,
                    frame.sha256.iter().map(|b| format!("{b:02x}")).collect::<String>()
                );
                imported = Some(frame);
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    let status = relay_vm::status(&handle.id)?;
    let console = relay_vm::console_log(&handle.id)?;
    let (
        pc,
        physical_pc,
        instructions,
        x0,
        x1,
        x3,
        x30,
        lock_byte,
        writer_pc,
        writer_value,
        writer_bytes,
        prior_writer_pc,
        prior_writer_value,
        prior_writer_bytes,
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
    ) = relay_vm::static_cpu_progress(&handle.id)?;
    let (
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
    ) = relay_vm::static_device_progress(&handle.id)?;
    relay_vm::stop(&handle.id)?;
    if console.len() > emitted_console {
        eprint!("{}", String::from_utf8_lossy(&console[emitted_console..]));
    }
    eprintln!("Relay static CPU progress: {instructions} instructions at pc={pc:#x}/physical={physical_pc:#x}, x0={x0:#x}, x1={x1:#x}, x3={x3:#x}, x30={x30:#x}, lock={lock_byte:#x}, writer={writer_pc:#x}/{writer_value:#x}/{writer_bytes}, prior={prior_writer_pc:#x}/{prior_writer_value:#x}/{prior_writer_bytes}");
    eprintln!("Relay static abort progress: count={abort_count}, pc={last_abort_pc:#x}, insn={last_abort_insn:#010x}, far={last_abort_address:#x}, esr={last_abort_esr:#x}");
    eprintln!("Relay static abort predecessor: pc={last_abort_prior_pc:#x}, insn={last_abort_prior_insn:#010x}");
    eprintln!("Relay static abort registers: x0={last_abort_x0:#x}, x1={last_abort_x1:#x}, x2={last_abort_x2:#x}, x3={last_abort_x3:#x}, x30={last_abort_x30:#x}, sp={last_abort_sp:#x}");
    eprintln!("Relay static device progress: mmio reads={mmio_reads}, writes={mmio_writes}, pl011 writes={pl011_writes}, last read={last_mmio_read:#x}, last write={last_mmio_write:#x}");
    eprintln!("Relay static GIC progress: IAR reads={gicc_iar_reads}, spurious={gicc_spurious_reads}, EOIR writes={gicc_eoir_writes}, last IRQ={last_irq_ack}, last EOI={last_irq_eoi}");
    eprintln!("Relay static console queues: rx notify/complete={console_rx_notifications}/{console_rx_completions}, tx notify/complete={console_tx_notifications}/{console_tx_completions}");
    if let Some(failure) = console_failure(&console) {
        return Err(format!("Relay static smoke failed: {failure}").into());
    }
    if status == relay_vm::RelayVmStatus::Exited {
        return Err("Relay static guest exited before smoke deadline".into());
    }
    println!("Relay static guest remained live for {seconds}s");
    if let Some(ready) = authenticated {
        println!(
            "Relay authenticated ready machine={} unit={}",
            ready.machine_id, ready.unit_state
        );
    }
    if let Some(frame) = imported {
        println!(
            "Relay imported SHM frame {}x{} sha256={}",
            frame.width,
            frame.height,
            frame.sha256.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
    }
    Ok(())
}

fn resolve(directory: &std::path::Path, artifact: &mut GuestArtifact) {
    let path = std::path::Path::new(&artifact.path);
    if path.is_relative() {
        artifact.path = directory.join(path).display().to_string();
    }
}

/// Fatal guest failures are failures even while the panic loop keeps the
/// interpreter thread alive. This diagnostic is not authenticated readiness.
fn console_failure(console: &[u8]) -> Option<&'static str> {
    const FAILURES: [&str; 5] = [
        "Kernel panic - not syncing:",
        "symbol lookup error:",
        "error while loading shared libraries:",
        "Inconsistency detected by ld.so:",
        "Relay StaticCpu:",
    ];
    FAILURES.into_iter().find(|failure| {
        console
            .windows(failure.len())
            .any(|window| window == failure.as_bytes())
    })
}

#[cfg(test)]
mod tests {
    use super::console_failure;

    #[test]
    fn live_panic_loop_is_not_a_successful_smoke_test() {
        assert!(
            console_failure(b"[31.15] Kernel panic - not syncing: Attempted to kill init!")
                .is_some()
        );
    }

    #[test]
    fn stage_two_loader_corruption_fails_smoke() {
        assert!(
            console_failure(b"install: symbol lookup error: undefined symbol: broken").is_some()
        );
        assert!(
            console_failure(b"mount: error while loading shared libraries: libsystemd.so.0")
                .is_some()
        );
        assert!(console_failure(b"Inconsistency detected by ld.so: assertion failed").is_some());
    }

    #[test]
    fn normal_boot_and_readiness_text_do_not_hide_fatal_errors() {
        assert_eq!(
            console_failure(b"Run /init as init process\nWAWONA_RELAY_READY=1\n"),
            None
        );
        assert!(console_failure(b"WAWONA_RELAY_READY=1\nKernel panic - not syncing:").is_some());
        assert_eq!(console_failure(b""), None);
    }
}

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
    if arguments.next().is_some() {
        return Err("usage: static_smoke GUEST_DIRECTORY [SECONDS]".into());
    }

    let mut manifest: GuestManifest =
        serde_json::from_slice(&fs::read(guest_directory.join("manifest.json"))?)?;
    resolve(&guest_directory, &mut manifest.kernel);
    if let Some(initrd) = &mut manifest.initrd {
        resolve(&guest_directory, initrd);
    }
    resolve(&guest_directory, &mut manifest.rootfs);

    let spec = RelaySpec {
        kind: RelayKind::Vm,
        platform: RelayPlatform::Ios,
        artifact: ArtifactClass::ModeA,
        machine_id: Some("static-smoke".into()),
        image: None,
        memory_mb: Some((manifest.memory_bytes / (1024 * 1024)) as u32),
        guest_page_size: Some(manifest.page_size),
        guest: Some(manifest),
        resources: Some(RelayRuntimeResources {
            launcher: String::new(),
            state_directory: String::new(),
            guest_directory: Some(guest_directory.display().to_string()),
            trusted_guest_keys: Default::default(),
            allow_unsigned_guest: true,
        }),
        ios_hv_host: None,
    };
    let handle = relay_vm::start(&spec)?;
    println!("Relay static guest started: {}", handle.id);
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline
        && relay_vm::status(&handle.id)? == relay_vm::RelayVmStatus::Running
    {
        thread::sleep(Duration::from_millis(25));
    }
    let status = relay_vm::status(&handle.id)?;
    let console = relay_vm::console_log(&handle.id)?;
    relay_vm::stop(&handle.id)?;
    if !console.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&console));
    }
    if status == relay_vm::RelayVmStatus::Exited {
        return Err("Relay static guest exited before smoke deadline".into());
    }
    println!("Relay static guest remained live for {seconds}s");
    Ok(())
}

fn resolve(directory: &std::path::Path, artifact: &mut GuestArtifact) {
    let path = std::path::Path::new(&artifact.path);
    if path.is_relative() {
        artifact.path = directory.join(path).display().to_string();
    }
}

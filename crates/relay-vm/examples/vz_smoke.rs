use relay_core::{
    ArtifactClass, GuestManifest, RelayKind, RelayPlatform, RelayRuntimeResources, RelaySpec,
};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let launcher = required(&mut arguments, "launcher")?;
    let guest_directory = required(&mut arguments, "guest directory")?;
    let state_directory = required(&mut arguments, "state directory")?;
    if arguments.next().is_some() {
        return Err("usage: vz_smoke LAUNCHER GUEST_DIRECTORY STATE_DIRECTORY".into());
    }

    let manifest: GuestManifest =
        serde_json::from_slice(&fs::read(guest_directory.join("manifest.json"))?)?;
    let spec = RelaySpec {
        kind: RelayKind::Vm,
        platform: RelayPlatform::Macos,
        artifact: ArtifactClass::ModeA,
        machine_id: Some("vz-smoke".into()),
        image: None,
        memory_mb: Some((manifest.memory_bytes / (1024 * 1024)) as u32),
        guest_page_size: Some(manifest.page_size),
        guest: Some(manifest),
        resources: Some(RelayRuntimeResources {
            launcher: launcher.display().to_string(),
            state_directory: state_directory.display().to_string(),
            guest_directory: Some(guest_directory.display().to_string()),
            trusted_guest_keys: Default::default(),
            allow_unsigned_guest: true,
        }),
        ios_hv_host: None,
    };
    let handle = relay_vm::start(&spec)?;
    println!(
        "Relay guest ready: {} {}",
        handle.id,
        handle.wayland_endpoint.as_deref().unwrap_or("")
    );
    relay_vm::stop(&handle.id)?;
    println!("Relay guest stopped: {}", handle.id);
    Ok(())
}

fn required(
    arguments: &mut impl Iterator<Item = std::ffi::OsString>,
    name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name}").into())
}

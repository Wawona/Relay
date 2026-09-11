//! Boot NixOS under VZ with a slim OCI runtime bundle shared over virtiofs.
use relay_core::{
    ArtifactClass, GuestManifest, RelayKind, RelayPlatform, RelayRuntimeResources, RelaySpec,
};
use relay_oci::materialize_slim_bundle;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let launcher = required(&mut arguments, "launcher")?;
    let guest_directory = required(&mut arguments, "guest directory")?;
    let state_directory = required(&mut arguments, "state directory")?;
    if arguments.next().is_some() {
        return Err("usage: oci_smoke LAUNCHER GUEST_DIRECTORY STATE_DIRECTORY".into());
    }

    fs::create_dir_all(&state_directory)?;
    let bundle = materialize_slim_bundle(&state_directory.join("oci-bundles/oci-smoke"))?;
    let manifest: GuestManifest =
        serde_json::from_slice(&fs::read(guest_directory.join("manifest.json"))?)?;
    let spec = RelaySpec {
        kind: RelayKind::Container,
        platform: RelayPlatform::Macos,
        artifact: ArtifactClass::ModeA,
        machine_id: Some("oci-smoke".into()),
        image: Some(bundle.root.display().to_string()),
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
        "Relay OCI guest ready: {} {}",
        handle.id,
        handle.wayland_endpoint.as_deref().unwrap_or("")
    );
    let log = relay_vm::console_log(&handle.id)?;
    if !log.windows(b"WAWONA_RELAY_READY=1".len()).any(|w| w == b"WAWONA_RELAY_READY=1") {
        return Err("OCI guest console missing WAWONA_RELAY_READY=1".into());
    }
    relay_vm::stop(&handle.id)?;
    println!("Relay OCI guest stopped: {}", handle.id);
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

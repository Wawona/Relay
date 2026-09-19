use relay_core::{
    ArtifactClass, GuestArtifact, GuestManifest, RelayKind, RelayPlatform, RelayRuntimeResources,
    RelaySpec,
};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let guest_directory = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: static_smoke GUEST_DIRECTORY")?;
    if arguments.next().is_some() {
        return Err("usage: static_smoke GUEST_DIRECTORY".into());
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
    println!("Relay static guest ready: {}", handle.id);
    Ok(())
}

fn resolve(directory: &std::path::Path, artifact: &mut GuestArtifact) {
    let path = std::path::Path::new(&artifact.path);
    if path.is_relative() {
        artifact.path = directory.join(path).display().to_string();
    }
}

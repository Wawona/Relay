//! Host-side OCI runtime bundle for virtiofs share into the NixOS guest.

use crate::{blob_path, failed, validate_layout, Descriptor, Manifest};
use relay_core::RelayError;
use serde_json::json;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

const WHITEOUT_PREFIX: &str = ".wh.";
const WHITEOUT_OPAQUE: &str = ".wh..wh..opq";

/// Validated OCI image-layout unpacked into a crun-ready bundle directory:
/// `config.json` + `rootfs/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBundle {
    pub root: PathBuf,
    pub manifest_digest: String,
}

/// Unpack a local OCI image-layout into `destination` as an OCI runtime bundle.
pub fn materialize_runtime_bundle(
    layout_root: &Path,
    destination: &Path,
) -> Result<RuntimeBundle, RelayError> {
    let architecture = crate::target_architecture();
    let layout = validate_layout(layout_root, "linux", architecture)?;
    let manifest_bytes = fs::read(blob_path(layout_root, &layout.manifest_digest)?)
        .map_err(|error| RelayError::Failed(format!("cannot read OCI manifest blob: {error}")))?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| RelayError::Failed(format!("invalid OCI manifest: {error}")))?;
    let config_bytes = fs::read(blob_path(layout_root, &layout.config_digest)?)
        .map_err(|error| RelayError::Failed(format!("cannot read OCI config blob: {error}")))?;

    let rootfs = destination.join("rootfs");
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|error| {
            RelayError::Failed(format!("cannot clear OCI runtime bundle: {error}"))
        })?;
    }
    fs::create_dir_all(&rootfs)
        .map_err(|error| RelayError::Failed(format!("cannot create OCI rootfs: {error}")))?;

    for digest in &layout.layer_digests {
        let descriptor = manifest
            .layers
            .iter()
            .find(|layer| &layer.digest == digest)
            .ok_or_else(|| failed("OCI layer digest missing from manifest"))?;
        let bytes = fs::read(blob_path(layout_root, digest)?)
            .map_err(|error| RelayError::Failed(format!("cannot read OCI layer: {error}")))?;
        apply_layer(descriptor, &bytes, &rootfs)?;
    }

    let process = runtime_process_from_config(&config_bytes, false)?;
    write_runtime_config(destination, process, false)?;

    Ok(RuntimeBundle {
        root: destination.to_path_buf(),
        manifest_digest: layout.manifest_digest,
    })
}

/// Minimal readiness bundle. Guest bind-mounts `/bin` `/usr` `/nix` `/lib`
/// so crun can run a shell without a full image rootfs.
pub fn materialize_slim_bundle(destination: &Path) -> Result<RuntimeBundle, RelayError> {
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|error| {
            RelayError::Failed(format!("cannot clear slim OCI bundle: {error}"))
        })?;
    }
    fs::create_dir_all(destination.join("rootfs")).map_err(|error| {
        RelayError::Failed(format!("cannot create slim OCI rootfs: {error}"))
    })?;
    let process = json!({
        "terminal": false,
        "user": {"uid": 0, "gid": 0},
        "args": [
            "sh",
            "-c",
            "printf 'WAWONA_OCI_READY=1\\n' > /dev/console; printf 'WAWONA_OCI_READY=1\\n' > /dev/hvc0; sleep infinity"
        ],
        "env": [
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            "TERM=xterm"
        ],
        "cwd": "/"
    });
    write_runtime_config(destination, process, true)?;
    Ok(RuntimeBundle {
        root: destination.to_path_buf(),
        manifest_digest: "sha256:relay-slim-oci".into(),
    })
}

fn write_runtime_config(
    destination: &Path,
    process: serde_json::Value,
    slim_guest_binds: bool,
) -> Result<(), RelayError> {
    let mut mounts = vec![
        json!({"destination": "/proc", "type": "proc", "source": "proc"}),
        json!({"destination": "/dev", "type": "tmpfs", "source": "tmpfs", "options": ["nosuid","strictatime","mode=755","size=65536k"]}),
        json!({"destination": "/dev/pts", "type": "devpts", "source": "devpts", "options": ["nosuid","noexec","newinstance","ptmxmode=0666","mode=0620"]}),
        json!({"destination": "/dev/shm", "type": "tmpfs", "source": "shm", "options": ["nosuid","noexec","nodev","mode=1777","size=65536k"]}),
        json!({"destination": "/dev/mqueue", "type": "mqueue", "source": "mqueue", "options": ["nosuid","noexec","nodev"]}),
        json!({"destination": "/sys", "type": "sysfs", "source": "sysfs", "options": ["nosuid","noexec","nodev","ro"]}),
    ];
    if slim_guest_binds {
        for path in ["/bin", "/usr", "/lib", "/lib64", "/nix", "/run/current-system"] {
            mounts.push(json!({
                "destination": path,
                "type": "bind",
                "source": path,
                "options": ["rbind", "ro", "nosuid", "nodev"]
            }));
        }
    }
    let config = json!({
        "ociVersion": "1.0.2",
        "process": process,
        "root": {"path": "rootfs", "readonly": false},
        "hostname": "wawona-oci",
        "mounts": mounts,
        "linux": {
            "namespaces": [
                {"type": "pid"},
                {"type": "ipc"},
                {"type": "uts"},
                {"type": "mount"}
            ]
        }
    });
    fs::write(
        destination.join("config.json"),
        serde_json::to_vec_pretty(&config).map_err(|error| {
            RelayError::Failed(format!("cannot encode OCI runtime config: {error}"))
        })?,
    )
    .map_err(|error| RelayError::Failed(format!("cannot write OCI runtime config: {error}")))?;
    Ok(())
}

fn runtime_process_from_config(
    config_bytes: &[u8],
    force_ready_shell: bool,
) -> Result<serde_json::Value, RelayError> {
    #[derive(serde::Deserialize)]
    struct ProcessConfig {
        #[serde(default)]
        #[serde(rename = "Entrypoint")]
        entrypoint: Option<Vec<String>>,
        #[serde(default)]
        #[serde(rename = "Cmd")]
        cmd: Option<Vec<String>>,
        #[serde(default)]
        #[serde(rename = "Env")]
        env: Option<Vec<String>>,
        #[serde(default)]
        #[serde(rename = "WorkingDir")]
        working_dir: Option<String>,
        #[serde(default)]
        user: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct LooseConfig {
        #[serde(default)]
        config: Option<ProcessConfig>,
    }
    let loose: LooseConfig = serde_json::from_slice(config_bytes)
        .map_err(|error| RelayError::Failed(format!("invalid OCI image config process: {error}")))?;
    let cfg = loose.config.unwrap_or(ProcessConfig {
        entrypoint: None,
        cmd: None,
        env: None,
        working_dir: None,
        user: None,
    });
    let mut args = Vec::new();
    if force_ready_shell {
        args.push("sh".into());
        args.push("-c".into());
        args.push(
            "printf 'WAWONA_OCI_READY=1\\n' > /dev/console; printf 'WAWONA_OCI_READY=1\\n' > /dev/hvc0; sleep infinity"
                .into(),
        );
    } else {
        if let Some(entrypoint) = cfg.entrypoint {
            args.extend(entrypoint);
        }
        if let Some(cmd) = cfg.cmd {
            args.extend(cmd);
        }
        if args.is_empty() {
            args.push("sh".into());
            args.push("-c".into());
            args.push(
                "printf 'WAWONA_OCI_READY=1\\n' > /dev/console; printf 'WAWONA_OCI_READY=1\\n' > /dev/hvc0; sleep infinity"
                    .into(),
            );
        }
    }
    let mut env = cfg.env.unwrap_or_default();
    if !env.iter().any(|value| value.starts_with("PATH=")) {
        env.push("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into());
    }
    if !env.iter().any(|value| value.starts_with("TERM=")) {
        env.push("TERM=xterm".into());
    }
    let cwd = cfg
        .working_dir
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/".into());
    let (uid, gid) = parse_user(cfg.user.as_deref());
    Ok(json!({
        "terminal": false,
        "user": {"uid": uid, "gid": gid},
        "args": args,
        "env": env,
        "cwd": cwd
    }))
}

fn parse_user(user: Option<&str>) -> (u32, u32) {
    let Some(user) = user.filter(|value| !value.is_empty()) else {
        return (0, 0);
    };
    if let Ok(uid) = user.parse::<u32>() {
        return (uid, uid);
    }
    if let Some((uid, gid)) = user.split_once(':') {
        return (uid.parse().unwrap_or(0), gid.parse().unwrap_or(0));
    }
    (0, 0)
}

fn apply_layer(descriptor: &Descriptor, bytes: &[u8], rootfs: &Path) -> Result<(), RelayError> {
    let reader: Box<dyn Read> = if descriptor.media_type.ends_with("+gzip") {
        Box::new(flate2::read::GzDecoder::new(Cursor::new(bytes)))
    } else if descriptor.media_type.ends_with("+zstd") {
        Box::new(
            zstd::stream::read::Decoder::new(Cursor::new(bytes))
                .map_err(|error| RelayError::Failed(format!("invalid zstd OCI layer: {error}")))?,
        )
    } else {
        Box::new(Cursor::new(bytes))
    };
    let mut archive = tar::Archive::new(reader);
    archive.set_preserve_permissions(true);
    for entry in archive
        .entries()
        .map_err(|error| RelayError::Failed(format!("invalid OCI layer tar: {error}")))?
    {
        let mut entry = entry
            .map_err(|error| RelayError::Failed(format!("invalid OCI layer entry: {error}")))?;
        let path = entry
            .path()
            .map_err(|error| RelayError::Failed(format!("invalid OCI layer path: {error}")))?
            .into_owned();
        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        if file_name == WHITEOUT_OPAQUE {
            let dir = safe_join(rootfs, parent)?;
            if dir.is_dir() {
                for child in fs::read_dir(&dir).map_err(|error| {
                    RelayError::Failed(format!("cannot clear opaque OCI dir: {error}"))
                })? {
                    let child = child.map_err(|error| {
                        RelayError::Failed(format!("cannot clear opaque OCI dir: {error}"))
                    })?;
                    remove_path(&child.path())?;
                }
            }
            continue;
        }
        if let Some(target) = file_name.strip_prefix(WHITEOUT_PREFIX) {
            remove_path(&safe_join(rootfs, &parent.join(target))?)?;
            continue;
        }
        let dest = safe_join(rootfs, &path)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                RelayError::Failed(format!("cannot create OCI rootfs parents: {error}"))
            })?;
        }
        if entry.header().entry_type() != tar::EntryType::Directory && dest.exists() {
            remove_path(&dest)?;
        }
        entry.unpack(&dest).map_err(|error| {
            RelayError::Failed(format!("cannot unpack OCI layer entry: {error}"))
        })?;
    }
    Ok(())
}

fn safe_join(base: &Path, rel: &Path) -> Result<PathBuf, RelayError> {
    let mut out = base.to_path_buf();
    let mut components = rel.components().peekable();
    if matches!(components.peek(), Some(Component::RootDir)) {
        components.next();
    }
    for component in components {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(failed("OCI layer path escapes the rootfs"));
            }
        }
    }
    Ok(out)
}

fn remove_path(path: &Path) -> Result<(), RelayError> {
    if !path.exists() && fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| RelayError::Failed(format!("cannot inspect OCI path: {error}")))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|error| RelayError::Failed(format!("cannot remove OCI dir: {error}")))
    } else {
        fs::remove_file(path)
            .map_err(|error| RelayError::Failed(format!("cannot remove OCI file: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn slim_bundle_writes_config_and_rootfs() {
        let root = std::env::temp_dir().join(format!(
            "relay-oci-slim-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let bundle = materialize_slim_bundle(&root).unwrap();
        assert!(bundle.root.join("config.json").is_file());
        assert!(bundle.root.join("rootfs").is_dir());
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(bundle.root.join("config.json")).unwrap()).unwrap();
        assert_eq!(config["ociVersion"], "1.0.2");
        assert!(config["mounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|mount| mount["destination"] == "/nix"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn materialize_runtime_bundle_applies_gzip_layer() {
        let root = std::env::temp_dir().join(format!(
            "relay-oci-layout-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let dest = std::env::temp_dir().join(format!(
            "relay-oci-bundle-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("blobs/sha256")).unwrap();
        fs::write(root.join("oci-layout"), r#"{"imageLayoutVersion":"1.0.0"}"#).unwrap();

        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let mut header = tar::Header::new_gnu();
            header.set_path("hello.txt").unwrap();
            header.set_size(5);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, b"hello".as_slice()).unwrap();
            builder.finish().unwrap();
        }
        let mut gzip_bytes = Vec::new();
        {
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gzip_bytes, flate2::Compression::default());
            encoder.write_all(&tar_bytes).unwrap();
            encoder.finish().unwrap();
        }
        let layer_digest = format!("{:x}", Sha256::digest(&gzip_bytes));
        let diff_id = format!("{:x}", Sha256::digest(&tar_bytes));
        fs::write(root.join("blobs/sha256").join(&layer_digest), &gzip_bytes).unwrap();

        let config = json!({
            "architecture": crate::target_architecture(),
            "os": "linux",
            "rootfs": {"type": "layers", "diff_ids": [format!("sha256:{diff_id}")]},
            "config": {
                "Cmd": ["echo", "hi"],
                "Env": ["PATH=/bin"]
            }
        });
        let config_bytes = serde_json::to_vec(&config).unwrap();
        let config_digest = format!("{:x}", Sha256::digest(&config_bytes));
        fs::write(root.join("blobs/sha256").join(&config_digest), &config_bytes).unwrap();

        let manifest = json!({
            "schemaVersion": 2,
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": format!("sha256:{config_digest}"),
                "size": config_bytes.len()
            },
            "layers": [{
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": format!("sha256:{layer_digest}"),
                "size": gzip_bytes.len()
            }]
        });
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let manifest_digest = format!("{:x}", Sha256::digest(&manifest_bytes));
        fs::write(
            root.join("blobs/sha256").join(&manifest_digest),
            &manifest_bytes,
        )
        .unwrap();
        let index = json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [{
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": format!("sha256:{manifest_digest}"),
                "size": manifest_bytes.len(),
                "platform": {
                    "architecture": crate::target_architecture(),
                    "os": "linux"
                }
            }]
        });
        fs::write(root.join("index.json"), serde_json::to_vec(&index).unwrap()).unwrap();

        let bundle = materialize_runtime_bundle(&root, &dest).unwrap();
        assert_eq!(
            fs::read_to_string(bundle.root.join("rootfs/hello.txt")).unwrap(),
            "hello"
        );
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(dest);
    }
}

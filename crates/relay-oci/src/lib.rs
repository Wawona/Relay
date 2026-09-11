//! OCI image validation and host-side runtime-bundle materialize.
//! Execution is always container-in-VM (`relay-vm` + guest crun).

mod materialize;

pub use materialize::{materialize_runtime_bundle, materialize_slim_bundle, RuntimeBundle};

use relay_core::{RelayError, RelayKind, RelaySpec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

const OCI_LAYOUT_VERSION: &str = "1.0.0";
pub(crate) const OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";
pub(crate) const OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
pub(crate) const OCI_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
const OCI_LAYER_PREFIX: &str = "application/vnd.oci.image.layer.v1.tar";
pub(crate) const DOCKER_CONFIG: &str = "application/vnd.docker.container.image.v1+json";
const DOCKER_LAYER_PREFIX: &str = "application/vnd.docker.image.rootfs.diff.tar";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedLayout {
    pub root: PathBuf,
    pub manifest_digest: String,
    pub config_digest: String,
    pub layer_digests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciBlob {
    pub digest: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestImagePlan {
    pub version: u32,
    pub manifest_digest: String,
    pub config_digest: String,
    pub blobs: Vec<OciBlob>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "command")]
pub enum ContainerCommand {
    Import { image: GuestImagePlan },
    Create { manifest_digest: String },
    Start,
    Stop,
    Logs { cursor: u64 },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerRequest {
    pub version: u32,
    pub request_id: u64,
    pub container_id: String,
    #[serde(flatten)]
    pub command: ContainerCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobChunk {
    pub digest: String,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub eof: bool,
}

const CONTAINER_PROTOCOL_VERSION: u32 = 1;
const MAX_CONTROL_FRAME: usize = 1024 * 1024;
const MAX_BLOB_CHUNK: usize = 1024 * 1024;

impl ValidatedLayout {
    pub fn guest_image_plan(&self) -> Result<GuestImagePlan, RelayError> {
        let mut digests = Vec::with_capacity(self.layer_digests.len() + 2);
        digests.push(self.manifest_digest.clone());
        digests.push(self.config_digest.clone());
        digests.extend(self.layer_digests.iter().cloned());
        let blobs = digests
            .into_iter()
            .map(|digest| {
                let path = blob_path(&self.root, &digest)?;
                let bytes = fs::metadata(&path)
                    .map_err(|error| {
                        RelayError::Failed(format!(
                            "cannot stat OCI delivery blob {}: {error}",
                            path.display()
                        ))
                    })?
                    .len();
                Ok(OciBlob { digest, bytes })
            })
            .collect::<Result<Vec<_>, RelayError>>()?;
        Ok(GuestImagePlan {
            version: CONTAINER_PROTOCOL_VERSION,
            manifest_digest: self.manifest_digest.clone(),
            config_digest: self.config_digest.clone(),
            blobs,
        })
    }

    pub fn read_blob_chunk(
        &self,
        digest: &str,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<BlobChunk, RelayError> {
        if maximum_bytes == 0 || maximum_bytes > MAX_BLOB_CHUNK {
            return Err(failed("OCI blob chunk size is outside Relay limits"));
        }
        let plan = self.guest_image_plan()?;
        let blob = plan
            .blobs
            .iter()
            .find(|blob| blob.digest == digest)
            .ok_or_else(|| failed("OCI blob is not part of the validated image"))?;
        if offset > blob.bytes {
            return Err(failed("OCI blob offset is out of range"));
        }
        let path = blob_path(&self.root, digest)?;
        let mut file = fs::File::open(&path).map_err(|error| {
            RelayError::Failed(format!(
                "cannot open OCI delivery blob {}: {error}",
                path.display()
            ))
        })?;
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(offset)).map_err(|error| {
            RelayError::Failed(format!("cannot seek OCI delivery blob: {error}"))
        })?;
        let remaining = blob.bytes - offset;
        let length = remaining.min(maximum_bytes as u64) as usize;
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes).map_err(|error| {
            RelayError::Failed(format!("cannot read OCI delivery blob: {error}"))
        })?;
        Ok(BlobChunk {
            digest: digest.to_string(),
            offset,
            eof: offset + length as u64 == blob.bytes,
            bytes,
        })
    }
}

impl ContainerRequest {
    pub fn new(request_id: u64, container_id: String, command: ContainerCommand) -> Self {
        Self {
            version: CONTAINER_PROTOCOL_VERSION,
            request_id,
            container_id,
            command,
        }
    }

    pub fn validate(&self) -> Result<(), RelayError> {
        if self.version != CONTAINER_PROTOCOL_VERSION {
            return Err(failed("unsupported Relay container protocol version"));
        }
        if self.container_id.is_empty()
            || self.container_id.len() > 128
            || !self
                .container_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(failed("Relay container id is invalid"));
        }
        if let ContainerCommand::Import { image } = &self.command {
            if image.version != CONTAINER_PROTOCOL_VERSION || image.blobs.is_empty() {
                return Err(failed("Relay guest image plan is invalid"));
            }
        }
        Ok(())
    }
}

pub fn encode_control(request: &ContainerRequest) -> Result<Vec<u8>, RelayError> {
    request.validate()?;
    let payload = serde_json::to_vec(request)
        .map_err(|error| RelayError::Failed(format!("cannot encode container request: {error}")))?;
    if payload.len() > MAX_CONTROL_FRAME {
        return Err(failed("Relay container control frame is too large"));
    }
    let mut frame = Vec::with_capacity(payload.len() + 4);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_control(frame: &[u8]) -> Result<ContainerRequest, RelayError> {
    let length = frame
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| failed("Relay container control frame is truncated"))?
        as usize;
    if length > MAX_CONTROL_FRAME || frame.len() != length + 4 {
        return Err(failed("Relay container control frame length is invalid"));
    }
    let request: ContainerRequest = serde_json::from_slice(&frame[4..])
        .map_err(|error| RelayError::Failed(format!("invalid container request: {error}")))?;
    request.validate()?;
    Ok(request)
}

#[derive(Deserialize)]
struct LayoutMarker {
    #[serde(rename = "imageLayoutVersion")]
    image_layout_version: String,
}

#[derive(Deserialize)]
struct Index {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "mediaType")]
    media_type: String,
    #[serde(default)]
    manifests: Vec<Descriptor>,
}

#[derive(Deserialize)]
pub(crate) struct Manifest {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    config: Descriptor,
    #[serde(default)]
    pub(crate) layers: Vec<Descriptor>,
}

#[derive(Deserialize)]
struct ImageConfig {
    architecture: String,
    os: String,
    rootfs: RootFs,
}

#[derive(Deserialize)]
struct RootFs {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    diff_ids: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct Descriptor {
    #[serde(rename = "mediaType")]
    pub(crate) media_type: String,
    pub(crate) digest: String,
    size: u64,
    #[serde(default)]
    platform: Option<Platform>,
}

#[derive(Deserialize)]
struct Platform {
    architecture: String,
    os: String,
    #[serde(default)]
    variant: Option<String>,
}

/// Resolve a container image input into a host path that `start_vz` can share
/// over virtiofs (`config.json` + `rootfs/`). Local OCI layouts are verified
/// and materialized; an empty image becomes a slim readiness bundle.
pub fn prepare_bundle(spec: &RelaySpec) -> Result<Option<String>, RelayError> {
    if spec.kind != RelayKind::Container {
        return Ok(None);
    }
    let image = spec.image.as_deref().unwrap_or("").trim();
    if image.starts_with("proot:") {
        return Err(RelayError::Forbidden(
            "proot is not a Relay container backend",
        ));
    }
    let state_root = spec
        .resources
        .as_ref()
        .map(|resources| PathBuf::from(&resources.state_directory))
        .unwrap_or_else(|| std::env::temp_dir().join("wawona-relay-oci"));
    let machine = spec
        .machine_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .unwrap_or("default");
    let destination = state_root.join("oci-bundles").join(machine);

    if image.is_empty() || image == "relay-slim-oci" {
        let bundle = materialize_slim_bundle(&destination)?;
        return Ok(Some(bundle.root.display().to_string()));
    }
    let path = Path::new(image);
    if path.join("config.json").is_file() && path.join("rootfs").is_dir() {
        return Ok(Some(path.display().to_string()));
    }
    if path.exists() {
        let architecture = target_architecture();
        let _layout = validate_layout(path, "linux", architecture)?;
        let bundle = materialize_runtime_bundle(path, &destination)?;
        return Ok(Some(bundle.root.display().to_string()));
    }
    Err(RelayError::Failed(format!(
        "OCI image is not a local layout or runtime bundle: {image}"
    )))
}

/// Validate an OCI image-layout directory, select an exact Linux platform,
/// and verify every referenced blob before guest delivery.
pub fn validate_layout(
    root: &Path,
    target_os: &str,
    target_architecture: &str,
) -> Result<ValidatedLayout, RelayError> {
    if !root.is_dir() {
        return Err(failed("OCI input is not an image-layout directory"));
    }
    let marker: LayoutMarker = read_json(&root.join("oci-layout"), "OCI layout marker")?;
    if marker.image_layout_version != OCI_LAYOUT_VERSION {
        return Err(failed("unsupported OCI image-layout version"));
    }
    let index: Index = read_json(&root.join("index.json"), "OCI index")?;
    if index.schema_version != 2 {
        return Err(failed("OCI index schemaVersion must be 2"));
    }
    if index.media_type != OCI_INDEX {
        return Err(failed("OCI index media type is unsupported"));
    }
    let descriptor = index
        .manifests
        .iter()
        .find(|descriptor| {
            descriptor.media_type == OCI_MANIFEST
                && descriptor.platform.as_ref().is_some_and(|platform| {
                    platform.os == target_os
                        && platform.architecture == target_architecture
                        && arm64_variant_matches(target_architecture, platform.variant.as_deref())
                })
        })
        .ok_or_else(|| failed("OCI index has no exact target platform manifest"))?;
    let manifest_bytes = verified_blob(root, descriptor)?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| RelayError::Failed(format!("invalid OCI manifest: {error}")))?;
    if manifest.schema_version != 2 {
        return Err(failed("OCI manifest schemaVersion must be 2"));
    }
    if manifest.config.media_type != OCI_CONFIG && manifest.config.media_type != DOCKER_CONFIG {
        return Err(failed("OCI manifest config media type is unsupported"));
    }
    let config_bytes = verified_blob(root, &manifest.config)?;
    let config: ImageConfig = serde_json::from_slice(&config_bytes)
        .map_err(|error| RelayError::Failed(format!("invalid OCI image config: {error}")))?;
    if config.os != target_os
        || config.architecture != target_architecture
        || config.rootfs.kind != "layers"
    {
        return Err(failed("OCI image config does not match target platform"));
    }
    if manifest.layers.is_empty() {
        return Err(failed("OCI manifest contains no filesystem layers"));
    }
    if config.rootfs.diff_ids.len() != manifest.layers.len() {
        return Err(failed("OCI config diff_ids do not match filesystem layers"));
    }
    for (layer, diff_id) in manifest.layers.iter().zip(&config.rootfs.diff_ids) {
        if !layer.media_type.starts_with(OCI_LAYER_PREFIX)
            && !layer.media_type.starts_with(DOCKER_LAYER_PREFIX)
        {
            return Err(failed("OCI manifest layer media type is unsupported"));
        }
        let bytes = verified_blob(root, layer)?;
        validate_layer(layer, &bytes, diff_id)?;
    }
    Ok(ValidatedLayout {
        root: root.to_path_buf(),
        manifest_digest: descriptor.digest.clone(),
        config_digest: manifest.config.digest,
        layer_digests: manifest
            .layers
            .into_iter()
            .map(|layer| layer.digest)
            .collect(),
    })
}

fn validate_layer(
    descriptor: &Descriptor,
    bytes: &[u8],
    expected_diff_id: &str,
) -> Result<(), RelayError> {
    let mut reader: Box<dyn Read> = if descriptor.media_type.ends_with("+gzip") {
        Box::new(flate2::read::GzDecoder::new(Cursor::new(bytes)))
    } else if descriptor.media_type.ends_with("+zstd") {
        Box::new(
            zstd::stream::read::Decoder::new(Cursor::new(bytes))
                .map_err(|error| RelayError::Failed(format!("invalid zstd OCI layer: {error}")))?,
        )
    } else {
        Box::new(Cursor::new(bytes))
    };
    let mut unpacked = Vec::new();
    reader
        .read_to_end(&mut unpacked)
        .map_err(|error| RelayError::Failed(format!("cannot decode OCI layer: {error}")))?;
    let expected = expected_diff_id
        .strip_prefix("sha256:")
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| failed("OCI config diff_id is not sha256"))?;
    let actual = format!("{:x}", Sha256::digest(&unpacked));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(failed(
            "OCI layer diff_id differs from uncompressed content",
        ));
    }

    let mut paths = BTreeSet::new();
    let mut archive = tar::Archive::new(Cursor::new(&unpacked));
    let entries = archive
        .entries()
        .map_err(|error| RelayError::Failed(format!("invalid OCI layer tar: {error}")))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| RelayError::Failed(format!("invalid OCI layer entry: {error}")))?;
        let path = entry
            .path()
            .map_err(|error| RelayError::Failed(format!("invalid OCI layer path: {error}")))?
            .into_owned();
        validate_archive_path(&path, "layer")?;
        if !paths.insert(path.clone()) {
            return Err(failed("OCI layer contains a duplicate path"));
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link()) {
            return Err(failed("OCI layer contains an unsupported special file"));
        }
        if kind.is_hard_link() {
            let target = entry
                .link_name()
                .map_err(|error| {
                    RelayError::Failed(format!("invalid OCI hard link target: {error}"))
                })?
                .ok_or_else(|| failed("OCI hard link has no target"))?;
            validate_archive_path(target.as_ref(), "hard link")?;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            if let Some(whiteout) = name.strip_prefix(".wh.") {
                if whiteout.is_empty() || (whiteout.starts_with(".wh.") && name != ".wh..wh..opq") {
                    return Err(failed("OCI layer contains an invalid whiteout"));
                }
                if !kind.is_file() {
                    return Err(failed("OCI whiteout must be a regular file"));
                }
            }
        }
    }
    Ok(())
}

fn validate_archive_path(path: &Path, name: &str) -> Result<(), RelayError> {
    use std::path::Component;

    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(RelayError::Failed(format!(
            "OCI {name} path escapes the container root"
        )));
    }
    Ok(())
}

fn verified_blob(root: &Path, descriptor: &Descriptor) -> Result<Vec<u8>, RelayError> {
    let path = blob_path(root, &descriptor.digest)?;
    let digest = descriptor.digest.strip_prefix("sha256:").unwrap();
    let bytes = fs::read(&path).map_err(|error| {
        RelayError::Failed(format!("cannot read OCI blob {}: {error}", path.display()))
    })?;
    if bytes.len() as u64 != descriptor.size {
        return Err(failed("OCI blob size differs from descriptor"));
    }
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(digest) {
        return Err(failed("OCI blob digest differs from descriptor"));
    }
    Ok(bytes)
}

pub(crate) fn blob_path(root: &Path, digest: &str) -> Result<PathBuf, RelayError> {
    let digest = digest
        .strip_prefix("sha256:")
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| failed("OCI descriptor digest is not sha256"))?;
    Ok(root.join("blobs").join("sha256").join(digest))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, name: &str) -> Result<T, RelayError> {
    let bytes = fs::read(path)
        .map_err(|error| RelayError::Failed(format!("cannot read {name}: {error}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| RelayError::Failed(format!("invalid {name}: {error}")))
}

pub(crate) fn target_architecture() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        architecture => architecture,
    }
}

fn arm64_variant_matches(architecture: &str, variant: Option<&str>) -> bool {
    architecture != "arm64" || variant.is_none_or(|variant| variant == "v8")
}

pub(crate) fn failed(message: &str) -> RelayError {
    RelayError::Failed(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn put_blob(root: &Path, bytes: &[u8]) -> serde_json::Value {
        let digest = format!("{:x}", Sha256::digest(bytes));
        fs::write(root.join("blobs/sha256").join(&digest), bytes).unwrap();
        json!({
            "mediaType": OCI_CONFIG,
            "digest": format!("sha256:{digest}"),
            "size": bytes.len()
        })
    }

    fn layer_tar(path: &str, contents: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut bytes);
            let mut header = tar::Header::new_gnu();
            header.set_path(path).unwrap();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, contents).unwrap();
            builder.finish().unwrap();
        }
        bytes
    }

    fn fixture() -> (PathBuf, String) {
        let root = std::env::temp_dir().join(format!(
            "relay-oci-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("blobs/sha256")).unwrap();
        fs::write(
            root.join("oci-layout"),
            br#"{"imageLayoutVersion":"1.0.0"}"#,
        )
        .unwrap();
        let layer = layer_tar("usr/share/wawona.txt", b"standards-shaped layer");
        let diff_id = format!("sha256:{:x}", Sha256::digest(&layer));
        let config = serde_json::to_vec(&json!({
            "architecture": "arm64",
            "os": "linux",
            "config": {},
            "rootfs": {"type": "layers", "diff_ids": [diff_id]}
        }))
        .unwrap();
        let config_descriptor = put_blob(&root, &config);
        let mut layer_descriptor = put_blob(&root, &layer);
        layer_descriptor["mediaType"] = json!("application/vnd.oci.image.layer.v1.tar");
        let manifest = serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "mediaType": OCI_MANIFEST,
            "config": config_descriptor,
            "layers": [layer_descriptor]
        }))
        .unwrap();
        let manifest_digest = format!("{:x}", Sha256::digest(&manifest));
        fs::write(root.join("blobs/sha256").join(&manifest_digest), &manifest).unwrap();
        fs::write(
            root.join("index.json"),
            serde_json::to_vec(&json!({
                "schemaVersion": 2,
                "mediaType": OCI_INDEX,
                "manifests": [{
                    "mediaType": OCI_MANIFEST,
                    "digest": format!("sha256:{manifest_digest}"),
                    "size": manifest.len(),
                    "platform": {"architecture":"arm64","os":"linux","variant":"v8"}
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        (root, manifest_digest)
    }

    #[test]
    fn validates_exact_platform_and_every_blob() {
        let (root, manifest_digest) = fixture();
        let layout = validate_layout(&root, "linux", "arm64").unwrap();
        assert_eq!(layout.manifest_digest, format!("sha256:{manifest_digest}"));
        assert_eq!(layout.layer_digests.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_platform_fallback_and_blob_mutation() {
        let (root, manifest_digest) = fixture();
        assert!(validate_layout(&root, "linux", "amd64").is_err());
        fs::write(root.join("blobs/sha256").join(manifest_digest), b"changed").unwrap();
        assert!(validate_layout(&root, "linux", "arm64").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn container_policy_rejects_proot() {
        let spec = RelaySpec {
            kind: RelayKind::Container,
            platform: relay_core::RelayPlatform::Ios,
            artifact: relay_core::ArtifactClass::ModeA,
            machine_id: None,
            image: Some("proot:alpine".into()),
            memory_mb: None,
            guest_page_size: None,
            guest: None,
            resources: None,
            ios_hv_host: None,
        };
        assert!(matches!(
            prepare_bundle(&spec),
            Err(RelayError::Forbidden(_))
        ));
    }

    #[test]
    fn empty_image_materializes_slim_runtime_bundle() {
        let root = std::env::temp_dir().join(format!(
            "relay-oci-prepare-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let spec = RelaySpec {
            kind: RelayKind::Container,
            platform: relay_core::RelayPlatform::Macos,
            artifact: relay_core::ArtifactClass::ModeA,
            machine_id: Some("slim".into()),
            image: None,
            memory_mb: None,
            guest_page_size: None,
            guest: None,
            resources: Some(relay_core::RelayRuntimeResources {
                launcher: "unused".into(),
                state_directory: root.display().to_string(),
                guest_directory: None,
                trusted_guest_keys: Default::default(),
                allow_unsigned_guest: true,
            }),
            ios_hv_host: None,
        };
        let path = prepare_bundle(&spec).unwrap().unwrap();
        assert!(Path::new(&path).join("config.json").is_file());
        assert!(Path::new(&path).join("rootfs").is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validates_whiteouts_and_rejects_special_files() {
        let whiteout = layer_tar("etc/.wh.obsolete", b"");
        let descriptor = Descriptor {
            media_type: "application/vnd.oci.image.layer.v1.tar".into(),
            digest: format!("sha256:{:x}", Sha256::digest(&whiteout)),
            size: whiteout.len() as u64,
            platform: None,
        };
        let diff_id = format!("sha256:{:x}", Sha256::digest(&whiteout));
        validate_layer(&descriptor, &whiteout, &diff_id).unwrap();

        let invalid = layer_tar("etc/.wh.", b"");
        let invalid_descriptor = Descriptor {
            size: invalid.len() as u64,
            digest: format!("sha256:{:x}", Sha256::digest(&invalid)),
            ..descriptor
        };
        let invalid_diff_id = format!("sha256:{:x}", Sha256::digest(&invalid));
        assert!(validate_layer(&invalid_descriptor, &invalid, &invalid_diff_id).is_err());
    }

    #[test]
    fn frames_guest_lifecycle_and_streams_only_validated_blobs() {
        let (root, _) = fixture();
        let layout = validate_layout(&root, "linux", "arm64").unwrap();
        let image = layout.guest_image_plan().unwrap();
        assert_eq!(image.blobs.len(), 3);
        let request = ContainerRequest::new(
            7,
            "hello-oci".into(),
            ContainerCommand::Import {
                image: image.clone(),
            },
        );
        let frame = encode_control(&request).unwrap();
        assert_eq!(decode_control(&frame).unwrap(), request);
        let manifest = image
            .blobs
            .iter()
            .find(|blob| blob.digest == image.manifest_digest)
            .unwrap();
        let chunk = layout
            .read_blob_chunk(&manifest.digest, 0, MAX_BLOB_CHUNK)
            .unwrap();
        assert!(chunk.eof);
        assert_eq!(chunk.bytes.len() as u64, manifest.bytes);
        assert!(layout
            .read_blob_chunk(&format!("sha256:{}", "0".repeat(64)), 0, 4096)
            .is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

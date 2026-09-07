//! OCI image management. Execution is always container-in-VM (`relay-vm`).

use relay_core::{RelayError, RelayKind, RelaySpec};

/// Unpack is userspace. Run is `relay-vm` on the same backend as `virtual_machine`.
pub fn prepare_bundle(spec: &RelaySpec) -> Result<Option<String>, RelayError> {
    if spec.kind != RelayKind::Container {
        return Ok(None);
    }
    let image = spec
        .image
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RelayError::Failed("container spec needs an OCI image ref".into()))?;
    // Full pull/unpack lives in import/containers (wwn-oci). This crate owns
    // the product rule: never proot, never host Docker.
    if image.starts_with("proot:") {
        return Err(RelayError::Forbidden("proot is not a Relay container backend"));
    }
    Ok(Some(image.to_string()))
}

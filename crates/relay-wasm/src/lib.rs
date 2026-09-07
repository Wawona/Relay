//! Mode A WASI. The archive is still built from `import/wasm` (`wawona-wasm`).

use relay_core::{RelayBackend, RelayError, RelayKind, RelaySpec, resolve_backend};

pub fn resolve(spec: &RelaySpec) -> Result<RelayBackend, RelayError> {
    if spec.kind != RelayKind::Wasm {
        return Err(RelayError::Failed("relay-wasm only handles kind=wasm".into()));
    }
    resolve_backend(spec)
}

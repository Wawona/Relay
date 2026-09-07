//! Wawona Runtime package manager (`wpm`).
//!
//! Local content-addressed store + optional Mode A registry
//! (`https://repo.wawona.io/wasm/v1`). No Mach-O, no `.deb`, no container pull.

pub mod cli;
pub mod ffi;
pub mod store;

#[cfg(feature = "registry")]
pub mod registry;

pub use store::{InstalledPackage, PackageStore};

/// Default Mode A Wasm registry (never the jailbreak APT tree).
pub const DEFAULT_REGISTRY: &str = "https://repo.wawona.io/wasm/v1";

/// Env override for store root (`Application Support/…/wasm-packages`).
pub const STORE_ENV: &str = "WAWONA_WASM_STORE";

/// Env override for registry base URL (Wasm channel only).
pub const REGISTRY_ENV: &str = "WAWONA_WASM_REGISTRY";

pub use cli::cli_run;

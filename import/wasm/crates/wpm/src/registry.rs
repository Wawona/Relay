//! Mode A registry client for `https://repo.wawona.io/wasm/v1`.
//!
//! Hard firewall: refuse jailbreak APT paths and non-Wasm URLs.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::store::PackageStore;
use crate::{DEFAULT_REGISTRY, REGISTRY_ENV};

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryIndex {
    pub schema: Option<u32>,
    pub packages: Vec<RegistryPackage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryPackage {
    pub name: String,
    pub version: String,
    pub digest: String,
    /// Relative to registry base, e.g. `packages/hello/0.1.0/component.wasm`
    pub url: String,
    pub wasi: Option<String>,
    pub summary: Option<String>,
}

pub struct RegistryClient {
    base: String,
}

impl RegistryClient {
    pub fn from_env_or_default() -> Result<Self> {
        let base = std::env::var(REGISTRY_ENV)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_REGISTRY.to_string());
        Self::new(&base)
    }

    pub fn new(base: &str) -> Result<Self> {
        assert_wasm_registry_url(base)?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn fetch_index(&self) -> Result<RegistryIndex> {
        let url = format!("{}/index.json", self.base);
        assert_wasm_registry_url(&url)?;
        let body = ureq::get(&url)
            .call()
            .with_context(|| format!("GET {url}"))?
            .into_string()
            .context("read index body")?;
        let index: RegistryIndex = serde_json::from_str(&body).context("parse index.json")?;
        Ok(index)
    }

    pub fn search(&self, query: &str) -> Result<Vec<RegistryPackage>> {
        let index = self.fetch_index()?;
        let q = query.to_ascii_lowercase();
        Ok(index
            .packages
            .into_iter()
            .filter(|p| {
                q.is_empty()
                    || p.name.to_ascii_lowercase().contains(&q)
                    || p.summary
                        .as_ref()
                        .map(|s| s.to_ascii_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .collect())
    }

    pub fn install(&self, store: &PackageStore, name: &str, version: Option<&str>) -> Result<()> {
        let index = self.fetch_index()?;
        let mut candidates: Vec<_> = index.packages.into_iter().filter(|p| p.name == name).collect();
        if candidates.is_empty() {
            bail!("package not found in registry: {name}");
        }
        if let Some(v) = version {
            candidates.retain(|p| p.version == v);
            if candidates.is_empty() {
                bail!("version {v} not found for {name}");
            }
        }
        // Prefer highest version string as listed order; take last matching.
        let pkg = candidates.pop().unwrap();
        let url = if pkg.url.starts_with("https://") || pkg.url.starts_with("http://") {
            pkg.url.clone()
        } else {
            format!("{}/{}", self.base, pkg.url.trim_start_matches('/'))
        };
        assert_wasm_registry_url(&url)?;
        if !url.contains(".wasm") && !url.ends_with("component.wasm") {
            // Allow digest URLs under /wasm/ packages path.
            if !url.contains("/wasm/") {
                bail!("refusing non-Wasm download URL: {url}");
            }
        }

        let mut reader = ureq::get(&url)
            .call()
            .with_context(|| format!("GET {url}"))?
            .into_reader();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut bytes).context("download wasm")?;
        store.install_bytes(&pkg.name, &pkg.version, &bytes, &pkg.digest, "registry")?;
        Ok(())
    }
}

/// Reject jailbreak APT and non-Wasm registry bases.
pub fn assert_wasm_registry_url(url: &str) -> Result<()> {
    let lower = url.to_ascii_lowercase();
    if lower.contains("/jailbreak")
        || lower.contains(".deb")
        || lower.contains("/dists/")
        || lower.contains("packages.gz")
        || lower.contains("sileo")
    {
        bail!("refusing jailbreak/APT URL (Mode A Wasm registry only): {url}");
    }
    if lower.starts_with("https://") || lower.starts_with("http://") {
        if !(lower.contains("/wasm") || lower.contains("localhost") || lower.contains("127.0.0.1"))
        {
            // Allow only paths that look like the Wasm channel, or local test servers.
            if !lower.contains("repo.wawona.io") {
                bail!("registry URL must be repo.wawona.io/wasm or explicit /wasm/ path: {url}");
            }
            if !lower.contains("/wasm") {
                bail!("repo.wawona.io URL must include /wasm/: {url}");
            }
        }
        return Ok(());
    }
    // Relative paths ok when composing.
    if url.starts_with('/') || !url.contains("://") {
        return Ok(());
    }
    bail!("unsupported registry URL: {url}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firewall_rejects_jailbreak() {
        assert!(assert_wasm_registry_url("https://repo.wawona.io/jailbreak/").is_err());
        assert!(assert_wasm_registry_url("https://repo.wawona.io/wasm/v1").is_ok());
    }
}

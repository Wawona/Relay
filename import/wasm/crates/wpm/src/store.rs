//! Local package store under `$WAWONA_WASM_STORE` or platform default.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::STORE_ENV;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    /// `sha256:<hex>` of the .wasm blob.
    pub digest: String,
    /// `p1`, `p2`, or `unknown`.
    pub wasi: String,
    /// `local` or `registry`.
    pub source: String,
    pub installed_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct IndexFile {
    packages: BTreeMap<String, InstalledPackage>,
}

#[derive(Debug)]
pub struct PackageStore {
    root: PathBuf,
}

impl PackageStore {
    pub fn open_default() -> Result<Self> {
        Self::open(default_store_root()?)
    }

    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("blobs")).context("create blobs dir")?;
        fs::create_dir_all(root.join("links")).context("create links dir")?;
        let s = Self { root };
        if !s.index_path().is_file() {
            s.write_index(&IndexFile::default())?;
        }
        Ok(s)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("installed.json")
    }

    fn read_index(&self) -> Result<IndexFile> {
        let path = self.index_path();
        if !path.is_file() {
            return Ok(IndexFile::default());
        }
        let data = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str(&data).context("parse installed.json")
    }

    fn write_index(&self, index: &IndexFile) -> Result<()> {
        let path = self.index_path();
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(index)?;
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<InstalledPackage>> {
        let index = self.read_index()?;
        Ok(index.packages.into_values().collect())
    }

    pub fn get(&self, name: &str) -> Result<Option<InstalledPackage>> {
        Ok(self.read_index()?.packages.get(name).cloned())
    }

    /// Resolve an installed package name to its `.wasm` path.
    pub fn resolve_wasm(&self, name: &str) -> Result<PathBuf> {
        let pkg = self
            .get(name)?
            .with_context(|| format!("package not installed: {name}"))?;
        let path = self.blob_path(&pkg.digest);
        if !path.is_file() {
            bail!("missing blob for {name} ({})", pkg.digest);
        }
        Ok(path)
    }

    fn blob_path(&self, digest: &str) -> PathBuf {
        let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
        self.root.join("blobs").join(hex).join("component.wasm")
    }

    /// Install from a local `.wasm` file (Phase 1 sideload / Files drop register).
    pub fn install_local(
        &self,
        wasm_path: &Path,
        name: Option<&str>,
        version: &str,
    ) -> Result<InstalledPackage> {
        if !wasm_path.is_file() {
            bail!("not a file: {}", wasm_path.display());
        }
        let mut magic = [0u8; 4];
        File::open(wasm_path)?.read_exact(&mut magic)?;
        if &magic != b"\0asm" {
            bail!(
                "{} is not a Wasm module (missing \\0asm magic)",
                wasm_path.display()
            );
        }

        let (digest, bytes) = hash_file(wasm_path)?;
        let name = name
            .map(str::to_string)
            .unwrap_or_else(|| infer_name(wasm_path));
        validate_name(&name)?;

        let hex = digest.strip_prefix("sha256:").unwrap();
        let blob_dir = self.root.join("blobs").join(hex);
        fs::create_dir_all(&blob_dir)?;
        let dest = blob_dir.join("component.wasm");
        if !dest.is_file() {
            fs::write(&dest, &bytes)?;
        }

        let pkg = InstalledPackage {
            name: name.clone(),
            version: version.to_string(),
            digest: digest.clone(),
            wasi: detect_wasi_hint(&bytes),
            source: "local".into(),
            installed_at: now_secs(),
        };

        let mut index = self.read_index()?;
        index.packages.insert(name.clone(), pkg.clone());
        self.write_index(&index)?;

        let link = self.root.join("links").join(&name);
        let _ = fs::remove_file(&link);
        fs::write(&link, format!("{}\n", dest.display()))?;

        Ok(pkg)
    }

    /// Install bytes already fetched (registry), verifying digest.
    pub fn install_bytes(
        &self,
        name: &str,
        version: &str,
        bytes: &[u8],
        expected_digest: &str,
        source: &str,
    ) -> Result<InstalledPackage> {
        validate_name(name)?;
        if bytes.len() < 4 || &bytes[0..4] != b"\0asm" {
            bail!("downloaded blob is not Wasm");
        }
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
        let expected = normalize_digest(expected_digest);
        if digest != expected {
            bail!("digest mismatch: got {digest}, expected {expected}");
        }

        let hex = digest.strip_prefix("sha256:").unwrap();
        let blob_dir = self.root.join("blobs").join(hex);
        fs::create_dir_all(&blob_dir)?;
        let dest = blob_dir.join("component.wasm");
        fs::write(&dest, bytes)?;

        let pkg = InstalledPackage {
            name: name.to_string(),
            version: version.to_string(),
            digest: digest.clone(),
            wasi: detect_wasi_hint(bytes),
            source: source.to_string(),
            installed_at: now_secs(),
        };
        let mut index = self.read_index()?;
        index.packages.insert(name.to_string(), pkg.clone());
        self.write_index(&index)?;
        let link = self.root.join("links").join(name);
        let _ = fs::remove_file(&link);
        fs::write(&link, format!("{}\n", dest.display()))?;
        Ok(pkg)
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        let mut index = self.read_index()?;
        let pkg = index
            .packages
            .remove(name)
            .with_context(|| format!("package not installed: {name}"))?;
        self.write_index(&index)?;
        let _ = fs::remove_file(self.root.join("links").join(name));

        let still = index.packages.values().any(|p| p.digest == pkg.digest);
        if !still {
            let hex = pkg.digest.strip_prefix("sha256:").unwrap_or(&pkg.digest);
            let _ = fs::remove_dir_all(self.root.join("blobs").join(hex));
        }
        Ok(())
    }
}

fn normalize_digest(d: &str) -> String {
    if d.starts_with("sha256:") {
        d.to_string()
    } else {
        format!("sha256:{d}")
    }
}

fn default_store_root() -> Result<PathBuf> {
    if let Ok(p) = std::env::var(STORE_ENV) {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let apple = PathBuf::from(&home)
            .join("Library")
            .join("Application Support")
            .join("Wawona")
            .join("wasm-packages");
        if cfg!(target_os = "macos")
            || cfg!(target_os = "ios")
            || PathBuf::from(&home).join("Library").is_dir()
        {
            return Ok(apple);
        }
        return Ok(PathBuf::from(home).join(".local/share/wawona/wasm-packages"));
    }
    bail!("HOME or {STORE_ENV} required for package store");
}

fn hash_file(path: &Path) -> Result<(String, Vec<u8>)> {
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&buf)));
    Ok((digest, buf))
}

fn infer_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("package")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 128 {
        bail!("invalid package name");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid package name: {name}");
    }
    Ok(())
}

fn detect_wasi_hint(bytes: &[u8]) -> String {
    if bytes.len() >= 8 {
        let ver = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if ver == 1 {
            return "p1".into();
        }
        // Component binaries often use other version fields; treat non-1 as p2-ish.
        if ver != 1 {
            return "p2".into();
        }
    }
    "unknown".into()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tiny_wasm() -> Vec<u8> {
        let mut v = b"\0asm".to_vec();
        v.extend_from_slice(&[1, 0, 0, 0]);
        v
    }

    #[test]
    fn install_list_remove() {
        let dir = tempfile::tempdir().unwrap();
        let store = PackageStore::open(dir.path()).unwrap();
        let wasm = dir.path().join("hello.wasm");
        File::create(&wasm).unwrap().write_all(&tiny_wasm()).unwrap();

        let pkg = store.install_local(&wasm, Some("hello"), "0.1.0").unwrap();
        assert_eq!(pkg.name, "hello");
        assert!(pkg.digest.starts_with("sha256:"));

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        let path = store.resolve_wasm("hello").unwrap();
        assert!(path.is_file());

        store.remove("hello").unwrap();
        assert!(store.list().unwrap().is_empty());
    }
}

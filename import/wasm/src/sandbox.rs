//! Filesystem preopen: one directory, `..` escape denied.

use std::path::{Path, PathBuf};

pub fn sandbox_root() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home);
        }
    }
    if let Ok(root) = std::env::var("WAWONA_ROOTFS") {
        let p = PathBuf::from(root).join("home");
        if p.is_dir() {
            return p;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Canonicalize `guest` against `root`. Deny NUL and path escape.
pub fn resolve_in_sandbox(root: &Path, guest: &str) -> Result<PathBuf, String> {
    if guest.contains('\0') {
        return Err("NUL in path".into());
    }
    let raw = if guest == "~" || guest == "~/" {
        root.to_path_buf()
    } else if let Some(rest) = guest.strip_prefix("~/") {
        root.join(rest)
    } else if guest.starts_with('/') {
        root.join(guest.trim_start_matches('/'))
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| root.to_path_buf())
            .join(guest)
    };
    let canon = std::fs::canonicalize(&raw).unwrap_or(raw);
    let root_canon = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    if !canon.starts_with(&root_canon) {
        return Err("EACCES: path escapes WASM sandbox".into());
    }
    Ok(canon)
}

pub fn is_wasm_magic(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.len() >= 4 && bytes[0] == 0x00 && bytes[1] == b'a' && bytes[2] == b's' && bytes[3] == b'm'
}

/// Component binaries are still `\0asm` but the payload after the header
/// uses the component model. Wasmtime's `Component::from_file` vs `Module`
/// is the authoritative split; this is a cheap hint (layer section 0x00
/// with component preamble is detected by the engine).
pub fn looks_like_component(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    // WASM component: magic + version 0x0d000100 (component) is common.
    bytes.len() >= 8 && bytes[0] == 0x00 && bytes[1] == b'a' && bytes[2] == b's' && bytes[3] == b'm'
        && bytes[4] == 0x0d
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn nul_rejected() {
        let root = std::env::temp_dir();
        assert!(resolve_in_sandbox(&root, "foo\0bar").is_err());
    }

    #[test]
    fn parent_escape_denied() {
        let tmp = std::env::temp_dir();
        let root = tmp.join(format!("wwn-wasm-sandbox-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let err = resolve_in_sandbox(&root, "../outside.txt").unwrap_err();
        assert!(err.contains("EACCES"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn magic_detects_wasm() {
        let tmp = std::env::temp_dir().join(format!("wwn-wasm-magic-{}", std::process::id()));
        fs::write(&tmp, b"\0asm\x01\x00\x00\x00").unwrap();
        assert!(is_wasm_magic(&tmp));
        assert!(!looks_like_component(&tmp));
        fs::write(&tmp, b"\0asm\x0d\x00\x01\x00").unwrap();
        assert!(looks_like_component(&tmp));
        let _ = fs::remove_file(&tmp);
    }
}

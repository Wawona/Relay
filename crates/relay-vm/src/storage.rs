//! Per-machine writable disks. Published files are never replaced or shrunk.
use relay_core::RelayError;
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

const GIB: u64 = 1 << 30;
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn capacity(base: u64, existing: u64, gib: u32, quota: u32) -> Option<u64> {
    let bytes = u64::from(gib) * GIB;
    (gib >= 4 && gib <= quota && quota <= 64 && base <= bytes && existing <= bytes).then_some(bytes)
}

/// Caller supplies an app-owned machine directory and a verified immutable base.
/// The returned file holds an exclusive advisory lock for the VM's lifetime.
pub(crate) fn open(
    base: &Path,
    destination: &Path,
    gib: u32,
    quota: u32,
) -> Result<File, RelayError> {
    let work = || -> io::Result<File> {
        let base_bytes = fs::metadata(base)?.len();
        let validate = |existing| {
            capacity(base_bytes, existing, gib, quota).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput,
                "disk capacity must be 4..64 GiB, within quota, and cannot shrink existing data")
            })
        };
        validate(0)?;
        let open_existing = || -> io::Result<File> {
            if !fs::symlink_metadata(destination)?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "VM disk must be a regular file",
                ));
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(destination)?;
            file.try_lock().map_err(io::Error::other)?;
            let bytes = file.metadata()?.len();
            if bytes < base_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "persisted disk is smaller than its base",
                ));
            }
            let target = validate(bytes)?;
            if target != bytes {
                file.set_len(target)?;
                file.sync_all()?;
            }
            Ok(file)
        };
        match fs::symlink_metadata(destination) {
            Ok(_) => return open_existing(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let staging = destination.with_extension(format!(
            "partial-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&staging)?;
        let result = (|| {
            file.try_lock().map_err(io::Error::other)?;
            let copied = io::copy(&mut File::open(base)?, &mut file)?;
            if copied != base_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "base changed while preparing disk",
                ));
            }
            file.set_len(validate(copied)?)?;
            file.sync_all()?;
            // Atomic no-replace publication: racing starts cannot replace data.
            fs::hard_link(&staging, destination)?;
            File::open(
                destination
                    .parent()
                    .ok_or_else(|| io::Error::other("disk has no parent"))?,
            )?
            .sync_all()?;
            Ok(file)
        })();
        let _ = fs::remove_file(staging);
        result
    };
    work().map_err(|e| RelayError::Failed(format!("cannot prepare VM disk: {e}")))
}

#[cfg(kani)]
#[kani::proof]
fn disk_capacity_preserves_existing_data_and_quota() {
    let base: u64 = kani::any();
    let existing: u64 = kani::any();
    let gib: u32 = kani::any();
    let quota: u32 = kani::any();
    if let Some(bytes) = capacity(base, existing, gib, quota) {
        assert!(bytes >= base && bytes >= existing);
        assert!(bytes >= 4 * GIB && bytes <= 64 * GIB);
        assert!(bytes <= u64::from(quota) * GIB);
        assert_eq!(bytes % GIB, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::FileExt;
    #[test]
    fn capacity_rejects_shrink_overflow_and_quota() {
        assert_eq!(capacity(1, 4 * GIB, 8, 64), Some(8 * GIB));
        for (base, old, target, quota) in [
            (u64::MAX, 0, 8, 64),
            (0, u64::MAX, 8, 64),
            (0, 8 * GIB, 4, 64),
            (0, 0, 65, 65),
            (0, 0, u32::MAX, 64),
            (0, 0, 4, 3),
        ] {
            assert_eq!(capacity(base, old, target, quota), None);
        }
    }
    #[test]
    fn persisted_disk_survives_restart_and_growth_with_exclusive_ownership() {
        let dir = std::env::temp_dir().join(format!(
            "relay-disk-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let base = dir.join("base");
        let disk = dir.join("disk");
        fs::write(&base, [7; 512]).unwrap();
        let file = open(&base, &disk, 4, 64).unwrap();
        let native_stop_guard = file.try_clone().unwrap();
        assert!(open(&base, &disk, 8, 64).is_err());
        assert_eq!(file.metadata().unwrap().len(), 4 * GIB);
        file.write_all_at(&[9; 512], 512).unwrap();
        file.sync_all().unwrap();
        drop(file);
        // CPU stop must not release machine ownership while a native worker
        // remains registered for a delayed Stop retry.
        assert!(open(&base, &disk, 8, 64).is_err());
        drop(native_stop_guard);
        let file = open(&base, &disk, 8, 64).unwrap();
        let mut data = [0; 512];
        file.read_exact_at(&mut data, 512).unwrap();
        assert_eq!(data, [9; 512]);
        file.read_exact_at(&mut data, 7 * GIB).unwrap();
        assert_eq!(data, [0; 512]);
        assert_eq!(fs::read(&base).unwrap(), [7; 512]);
        drop(file);
        assert!(open(&base, &disk, 4, 64).is_err());
        assert_eq!(fs::metadata(&disk).unwrap().len(), 8 * GIB);
        fs::remove_dir_all(dir).unwrap();
    }
}

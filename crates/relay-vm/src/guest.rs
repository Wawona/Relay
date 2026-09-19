//! Guest artifact loading. This is deliberately separate from the CPU proof:
//! an accepted NixOS manifest must never be mistaken for a successful boot.

use crate::dtb;
use crate::page_translate::PageTranslate;
use relay_core::{GuestArtifact, GuestManifest, GuestPageSize, HostPageSize, RelayError};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::PathBuf};

#[allow(dead_code)] // Staged now; consumed by the Linux boot loader next.
pub struct LoadedGuest {
    pub page_size: GuestPageSize,
    pub memory: GuestMemory,
    pub kernel: Vec<u8>,
    pub initrd: Option<Vec<u8>>,
    pub rootfs: Vec<u8>,
    pub rootfs_path: PathBuf,
}

/// AArch64 Linux entry registers and physical placements. Creating this state
/// does not execute the kernel: the CPU/MMU/device implementation must claim
/// that separately.
#[allow(dead_code)] // Consumed when static CPU boot handoff is implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxBootState {
    pub entry_pc: u64,
    pub dtb_address: u64,
    pub initrd_address: Option<u64>,
    pub command_line: String,
    pub dtb_bytes: usize,
}

#[allow(dead_code)]
const DTB_ADDRESS: u64 = 0x40000;
#[allow(dead_code)]
const KERNEL_ADDRESS: u64 = 0x80000;

/// Stage verified kernel bytes and optional initrd at deterministic AArch64
/// Linux physical addresses. The caller must still provide a valid DTB before
/// it can transfer control to `entry_pc`.
#[allow(dead_code)] // Staged separately so a manifest is never mistaken for boot.
pub fn prepare_linux_boot(
    manifest: &GuestManifest,
) -> Result<(LoadedGuest, LinuxBootState), RelayError> {
    // The immutable rootfs is a virtio-block backing store, never a second
    // in-RAM copy.  Verify it before allocating RAM, then stage only the
    // executable boot artifacts into the single GuestMemory arena.
    let page_size = manifest.validate()?;
    let kernel = read_artifact("kernel", &manifest.kernel)?;
    let initrd = manifest
        .initrd
        .as_ref()
        .map(|artifact| read_artifact("initrd", artifact))
        .transpose()?;
    verify_artifact("rootfs", &manifest.rootfs)?;
    let mut guest = LoadedGuest {
        page_size,
        memory: GuestMemory::allocate(page_size, manifest.memory_bytes)?,
        kernel,
        initrd,
        rootfs_path: PathBuf::from(&manifest.rootfs.path),
        // `start_ios` attaches this verified artifact through virtio-block.
        // Keeping it empty here prevents a rootfs-sized duplicate allocation.
        rootfs: Vec::new(),
    };
    let kernel_end = KERNEL_ADDRESS
        .checked_add(guest.kernel.len() as u64)
        .ok_or_else(|| RelayError::Failed("guest kernel placement overflow".into()))?;
    guest.memory.write(KERNEL_ADDRESS, &guest.kernel)?;
    let initrd_address = if let Some(initrd) = &guest.initrd {
        let address = align_up(kernel_end, guest.page_size.bytes() as u64)?;
        guest.memory.write(address, initrd)?;
        Some(address)
    } else {
        None
    };
    let initrd_range = match (initrd_address, guest.initrd.as_ref()) {
        (Some(start), Some(initrd)) => Some((
            start,
            start
                .checked_add(initrd.len() as u64)
                .ok_or_else(|| RelayError::Failed("guest initrd placement overflow".into()))?,
        )),
        _ => None,
    };
    let dtb = dtb::build(manifest.memory_bytes, &manifest.command_line, initrd_range)?;
    if DTB_ADDRESS + dtb.len() as u64 > KERNEL_ADDRESS {
        return Err(RelayError::Failed(
            "Relay DTB and kernel layout overlap".into(),
        ));
    }
    guest.memory.write(DTB_ADDRESS, &dtb)?;
    Ok((
        guest,
        LinuxBootState {
            entry_pc: KERNEL_ADDRESS,
            dtb_address: DTB_ADDRESS,
            initrd_address,
            command_line: manifest.command_line.clone(),
            dtb_bytes: dtb.len(),
        },
    ))
}

#[allow(dead_code)]
fn align_up(value: u64, alignment: u64) -> Result<u64, RelayError> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or_else(|| RelayError::Failed("guest address alignment overflow".into()))
}

/// Bounded guest physical memory. Virtual translation is MMU + [`PageTranslate`].
#[allow(dead_code)] // Used by the staged Linux boot loader; tests cover it now.
#[derive(Debug)]
pub struct GuestMemory {
    page_size: GuestPageSize,
    translate: PageTranslate,
    /// Host arena length (may be larger than guest_bytes when rounded).
    guest_bytes: usize,
    bytes: Vec<u8>,
}

#[allow(dead_code)] // Called by the staged Linux boot and MMU layers next.
impl GuestMemory {
    pub fn allocate(page_size: GuestPageSize, bytes: u64) -> Result<Self, RelayError> {
        Self::allocate_on_host(page_size, HostPageSize::detect(), bytes)
    }

    /// Allocate guest RAM mapped onto an explicit host page size.
    pub fn allocate_on_host(
        page_size: GuestPageSize,
        host: HostPageSize,
        bytes: u64,
    ) -> Result<Self, RelayError> {
        let translate = PageTranslate::new(page_size, host)?;
        let arena = translate.host_arena_bytes(bytes)?;
        let guest_len = usize::try_from(bytes)
            .map_err(|_| RelayError::Failed("guest memory does not fit this host".into()))?;
        let arena_len = usize::try_from(arena)
            .map_err(|_| RelayError::Failed("host arena does not fit this process".into()))?;
        Ok(Self {
            page_size,
            translate,
            guest_bytes: guest_len,
            bytes: vec![0; arena_len],
        })
    }

    pub fn len(&self) -> usize {
        self.guest_bytes
    }

    pub fn host_arena_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn page_size(&self) -> GuestPageSize {
        self.page_size
    }

    pub fn translate(&self) -> PageTranslate {
        self.translate
    }

    pub fn read(&self, address: u64, out: &mut [u8]) -> Result<(), RelayError> {
        let range = self.range(address, out.len())?;
        out.copy_from_slice(&self.bytes[range]);
        Ok(())
    }

    pub fn write(&mut self, address: u64, input: &[u8]) -> Result<(), RelayError> {
        let range = self.range(address, input.len())?;
        self.bytes[range].copy_from_slice(input);
        Ok(())
    }

    fn range(&self, address: u64, len: usize) -> Result<std::ops::Range<usize>, RelayError> {
        let offset = self.translate.guest_to_host_offset(address)?;
        let start = usize::try_from(offset)
            .map_err(|_| RelayError::Failed("guest address does not fit this host".into()))?;
        let end = start
            .checked_add(len)
            .filter(|end| *end <= self.guest_bytes && *end <= self.bytes.len())
            .ok_or_else(|| {
                RelayError::Failed(format!(
                    "guest physical memory access out of range address={address:#x} len={len} memory={:#x}",
                    self.guest_bytes
                ))
            })?;
        Ok(start..end)
    }
}

#[allow(dead_code)] // Called by the staged Linux boot path, not the proof path.
pub fn load(manifest: &GuestManifest) -> Result<LoadedGuest, RelayError> {
    let page_size = validate_artifacts(manifest)?;
    let kernel = read_artifact("kernel", &manifest.kernel)?;
    let initrd = manifest
        .initrd
        .as_ref()
        .map(|artifact| read_artifact("initrd", artifact))
        .transpose()?;
    let rootfs = read_artifact("rootfs", &manifest.rootfs)?;
    Ok(LoadedGuest {
        page_size,
        memory: GuestMemory::allocate(page_size, manifest.memory_bytes)?,
        kernel,
        initrd,
        rootfs,
        rootfs_path: PathBuf::from(&manifest.rootfs.path),
    })
}

/// Check bundle files without allocating guest RAM or loading a rootfs. The
/// static CPU start path uses this until it can actually hand ownership to a
/// Linux boot loader.
pub fn validate_artifacts(manifest: &GuestManifest) -> Result<GuestPageSize, RelayError> {
    let page_size = manifest.validate()?;
    verify_artifact("kernel", &manifest.kernel)?;
    verify_artifact("rootfs", &manifest.rootfs)?;
    if let Some(initrd) = &manifest.initrd {
        verify_artifact("initrd", initrd)?;
    }
    Ok(page_size)
}

#[allow(dead_code)] // Kept with `load` for the staged Linux boot path.
fn read_artifact(name: &str, artifact: &GuestArtifact) -> Result<Vec<u8>, RelayError> {
    let bytes = fs::read(&artifact.path)
        .map_err(|error| RelayError::Failed(format!("cannot read guest {name}: {error}")))?;
    verify_bytes(name, artifact, &bytes)?;
    Ok(bytes)
}

fn verify_artifact(name: &str, artifact: &GuestArtifact) -> Result<(), RelayError> {
    let file = fs::File::open(&artifact.path)
        .map_err(|error| RelayError::Failed(format!("cannot read guest {name}: {error}")))?;
    let size = file
        .metadata()
        .map_err(|error| RelayError::Failed(format!("cannot stat guest {name}: {error}")))?
        .len();
    if size != artifact.bytes {
        return Err(RelayError::Failed(format!(
            "guest {name} size differs from manifest"
        )));
    }
    let actual = sha256_reader(file)?;
    if !actual.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(RelayError::Failed(format!(
            "guest {name} sha256 differs from manifest"
        )));
    }
    Ok(())
}

fn verify_bytes(name: &str, artifact: &GuestArtifact, bytes: &[u8]) -> Result<(), RelayError> {
    if bytes.len() as u64 != artifact.bytes {
        return Err(RelayError::Failed(format!(
            "guest {name} size differs from manifest"
        )));
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(RelayError::Failed(format!(
            "guest {name} sha256 differs from manifest"
        )));
    }
    Ok(())
}

fn sha256_reader(mut reader: impl Read) -> Result<String, RelayError> {
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| RelayError::Failed(format!("cannot hash guest artifact: {error}")))?;
        if read == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn artifact(path: &std::path::Path, bytes: u64) -> GuestArtifact {
        GuestArtifact {
            path: path.display().to_string(),
            sha256: sha256_reader(fs::File::open(path).unwrap()).unwrap(),
            bytes,
        }
    }

    #[test]
    fn loads_page_aligned_guest_artifacts() {
        let dir = std::env::temp_dir().join(format!(
            "relay-guest-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let kernel = dir.join("Image");
        let rootfs = dir.join("rootfs");
        fs::write(&kernel, [1u8, 2, 3, 4]).unwrap();
        fs::write(&rootfs, [5u8, 6, 7, 8]).unwrap();
        let manifest = GuestManifest {
            version: GuestManifest::VERSION,
            page_size: 16384,
            memory_bytes: 16384 * 16,
            kernel: artifact(&kernel, 4),
            initrd: None,
            rootfs: artifact(&rootfs, 4),
            command_line: String::new(),
            compatibility: relay_core::GuestCompatibility::default(),
            signature: None,
        };
        let loaded = load(&manifest).unwrap();
        assert_eq!(loaded.page_size, GuestPageSize::SIXTEEN_KIB);
        assert_eq!(loaded.memory.len(), 16384 * 16);
        assert_eq!(loaded.memory.page_size(), GuestPageSize::SIXTEEN_KIB);
        assert_eq!(loaded.kernel, [1, 2, 3, 4]);
        assert_eq!(loaded.rootfs, [5, 6, 7, 8]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_artifact_size_drift_without_allocating_guest_memory() {
        let dir = std::env::temp_dir().join(format!(
            "relay-guest-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let kernel = dir.join("Image");
        let rootfs = dir.join("rootfs");
        fs::write(&kernel, [1u8; 4]).unwrap();
        fs::write(&rootfs, [2u8; 4]).unwrap();
        let manifest = GuestManifest {
            version: GuestManifest::VERSION,
            page_size: 4096,
            memory_bytes: 4096 * 16,
            kernel: artifact(&kernel, 5),
            initrd: None,
            rootfs: artifact(&rootfs, 4),
            command_line: String::new(),
            compatibility: relay_core::GuestCompatibility::default(),
            signature: None,
        };
        assert!(validate_artifacts(&manifest).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_artifact_hash_drift() {
        let dir = std::env::temp_dir().join(format!(
            "relay-guest-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let kernel = dir.join("Image");
        let rootfs = dir.join("rootfs");
        fs::write(&kernel, [1u8; 4]).unwrap();
        fs::write(&rootfs, [2u8; 4]).unwrap();
        let mut bad_kernel = artifact(&kernel, 4);
        bad_kernel.sha256 = "0".repeat(64);
        let manifest = GuestManifest {
            version: GuestManifest::VERSION,
            page_size: 4096,
            memory_bytes: 4096 * 16,
            kernel: bad_kernel,
            initrd: None,
            rootfs: artifact(&rootfs, 4),
            command_line: String::new(),
            compatibility: relay_core::GuestCompatibility::default(),
            signature: None,
        };
        assert!(validate_artifacts(&manifest).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn physical_memory_is_bounded_and_cross_page_safe() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096 * 2).unwrap();
        memory.write(4095, &[1, 2, 3]).unwrap();
        let mut out = [0; 3];
        memory.read(4095, &mut out).unwrap();
        assert_eq!(out, [1, 2, 3]);
        assert!(memory.write(8191, &[9, 9]).is_err());
    }

    #[test]
    fn guest_4k_on_host_16k_rounds_arena() {
        let memory = GuestMemory::allocate_on_host(
            GuestPageSize::FOUR_KIB,
            HostPageSize::SIXTEEN_KIB,
            4096 * 3,
        )
        .unwrap();
        assert_eq!(memory.len(), 4096 * 3);
        assert_eq!(memory.host_arena_len(), 16384);
        assert_eq!(memory.translate().pair_label(), "guest-4k-on-host-16k");
    }

    #[test]
    fn stages_linux_kernel_and_initrd_without_executing_them() {
        let dir = std::env::temp_dir().join(format!(
            "relay-guest-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let kernel = dir.join("Image");
        let initrd = dir.join("initrd");
        let rootfs = dir.join("rootfs");
        fs::write(&kernel, [1u8, 2, 3, 4]).unwrap();
        fs::write(&initrd, [5u8, 6, 7, 8]).unwrap();
        fs::write(&rootfs, [9u8; 4]).unwrap();
        let manifest = GuestManifest {
            version: GuestManifest::VERSION,
            page_size: 4096,
            memory_bytes: 2 * 1024 * 1024,
            kernel: artifact(&kernel, 4),
            initrd: Some(artifact(&initrd, 4)),
            rootfs: artifact(&rootfs, 4),
            command_line: "console=hvc0".into(),
            compatibility: relay_core::GuestCompatibility::default(),
            signature: None,
        };
        let (guest, state) = prepare_linux_boot(&manifest).unwrap();
        assert_eq!(state.entry_pc, KERNEL_ADDRESS);
        assert_eq!(state.initrd_address, Some(KERNEL_ADDRESS + 4096));
        assert!(state.dtb_bytes > 40);
        let mut loaded_kernel = [0; 4];
        guest
            .memory
            .read(KERNEL_ADDRESS, &mut loaded_kernel)
            .unwrap();
        assert_eq!(loaded_kernel, [1, 2, 3, 4]);
        assert!(guest.rootfs.is_empty(), "rootfs is virtio-block backed");
        fs::remove_dir_all(dir).unwrap();
    }
}

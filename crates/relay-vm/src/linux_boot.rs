//! ARM64 Linux Image validation and StaticCpu handoff.
//!
//! This module deliberately owns the point at which a verified artifact becomes
//! executable guest data.  It does not turn a probe frame into a boot result:
//! until the EL1 instruction set can execute an Image, it reports the exact
//! first word that still needs a handler.

use crate::{
    cpu::StaticCpu,
    guest::{self, LinuxBootState, GUEST_RAM_BASE},
};
use relay_core::{GuestArtifact, GuestManifest, RelayError};
use std::{fs::File, io::Read};

/// ARM64 Image magic at offset 0x38, encoded little-endian in the file.
const ARM64_IMAGE_MAGIC: u32 = 0x644d_5241;
const HEADER_BYTES: usize = 0x40;
const ARM64_IMAGE_RELOCATABLE: u64 = 1 << 3;
const KERNEL_BASE_ALIGNMENT: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageHeader {
    pub first_instruction: u32,
    pub text_offset: u64,
    pub image_size: u64,
    pub flags: u64,
}

pub fn read_image_header(kernel: &GuestArtifact) -> Result<ImageHeader, RelayError> {
    let mut file = File::open(&kernel.path)
        .map_err(|error| RelayError::Failed(format!("cannot read guest kernel: {error}")))?;
    let mut bytes = [0u8; HEADER_BYTES];
    file.read_exact(&mut bytes).map_err(|error| {
        RelayError::Failed(format!(
            "guest kernel is smaller than ARM64 Image header: {error}"
        ))
    })?;
    parse_image_header(&bytes)
}

fn parse_image_header(bytes: &[u8]) -> Result<ImageHeader, RelayError> {
    if bytes.len() < HEADER_BYTES {
        return Err(RelayError::Failed(
            "guest kernel is smaller than ARM64 Image header".into(),
        ));
    }
    let magic = u32::from_le_bytes(bytes[0x38..0x3c].try_into().unwrap());
    if magic != ARM64_IMAGE_MAGIC {
        return Err(RelayError::Failed(format!(
            "guest kernel is not an ARM64 Image (magic {magic:#010x})"
        )));
    }
    Ok(ImageHeader {
        first_instruction: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        text_offset: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        image_size: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        flags: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
    })
}

fn kernel_load_address(header: ImageHeader) -> Result<u64, RelayError> {
    if header.flags & ARM64_IMAGE_RELOCATABLE == 0 {
        return Err(RelayError::Failed(
            "Relay StaticCpu requires a relocatable ARM64 Image".into(),
        ));
    }
    GUEST_RAM_BASE
        .checked_add(KERNEL_BASE_ALIGNMENT)
        .and_then(|base| base.checked_add(header.text_offset))
        .ok_or_else(|| RelayError::Failed("guest kernel load address overflow".into()))
}

/// Verify, stage the Image/DTB/initrd into the one guest RAM arena, and report
/// a fail-closed instruction boundary until the real EL1 runner accepts it.
pub(crate) fn create_cpu(
    manifest: &GuestManifest,
) -> Result<(StaticCpu, LinuxBootState), RelayError> {
    create_cpu_with_disk(manifest, None)
}

pub(crate) fn create_cpu_with_disk(
    manifest: &GuestManifest,
    disk: Option<crate::virtio_block::BlockDevice>,
) -> Result<(StaticCpu, LinuxBootState), RelayError> {
    let header = read_image_header(&manifest.kernel)?;
    if header.first_instruction == 0 {
        return Err(RelayError::Failed(
            "ARM64 Image has an empty entry instruction".into(),
        ));
    }
    let kernel_address = kernel_load_address(header)?;
    let kernel_memory_bytes = header.image_size.max(manifest.kernel.bytes);
    let (guest, boot) = guest::prepare_linux_boot(manifest, kernel_address, kernel_memory_bytes)?;
    let rootfs_path = guest.rootfs_path.clone();
    let disk = match disk {
        Some(disk) => disk,
        None => crate::virtio_block::BlockDevice::from_file(&rootfs_path, 4)?,
    };
    let mut cpu = StaticCpu::new_with_block(guest.memory, boot.entry_pc, disk)?;
    // ARM64 Linux boot protocol: x0 is the physical DTB address, the other
    // argument registers are zero on entry.  Follow real Image control flow
    // until a handler is missing; do not return a placeholder frame.
    cpu.set_x(0, boot.dtb_address);
    Ok((cpu, boot))
}

pub fn prepare(manifest: &GuestManifest) -> Result<LinuxBootState, RelayError> {
    let (mut cpu, _) = create_cpu(manifest)?;
    match cpu.run(1_000_000) {
        Err(error) => Err(error),
        Ok(()) => unreachable!("bounded CPU run cannot complete without a stop reason"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_arm64_image_header() {
        let path = std::env::temp_dir().join(format!("relay-image-{}", std::process::id()));
        let mut bytes = [0u8; HEADER_BYTES];
        bytes[..4].copy_from_slice(&0x1400_0008u32.to_le_bytes());
        bytes[8..16].copy_from_slice(&0x80000u64.to_le_bytes());
        bytes[16..24].copy_from_slice(&0x200000u64.to_le_bytes());
        bytes[24..32].copy_from_slice(&ARM64_IMAGE_RELOCATABLE.to_le_bytes());
        bytes[0x38..0x3c].copy_from_slice(&ARM64_IMAGE_MAGIC.to_le_bytes());
        File::create(&path).unwrap().write_all(&bytes).unwrap();
        let artifact = GuestArtifact {
            path: path.display().to_string(),
            bytes: HEADER_BYTES as u64,
            sha256: "00".repeat(32),
        };
        let header = read_image_header(&artifact).unwrap();
        assert_eq!(header.first_instruction, 0x1400_0008);
        assert_eq!(header.text_offset, 0x80000);
        assert_eq!(kernel_load_address(header).unwrap(), 0x4028_0000);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn zero_text_offset_still_uses_aligned_relocatable_base() {
        let header = ImageHeader {
            first_instruction: 0x1400_0008,
            text_offset: 0,
            image_size: 0x3e00_000,
            flags: ARM64_IMAGE_RELOCATABLE,
        };
        assert_eq!(kernel_load_address(header).unwrap(), 0x4020_0000);
    }

    #[test]
    fn rejects_non_relocatable_image_for_safe_dtb_layout() {
        let header = ImageHeader {
            first_instruction: 0x1400_0008,
            text_offset: 0x80000,
            image_size: 0x20_0000,
            flags: 0,
        };
        assert!(kernel_load_address(header).is_err());
    }
}

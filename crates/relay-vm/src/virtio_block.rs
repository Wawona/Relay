//! Sparse virtio-block backend: immutable NixOS base plus writable overlay.

#![allow(dead_code)] // The static CPU bus will expose this device next.

use relay_core::{DiskResizePlan, RelayError};
use std::{collections::BTreeMap, fs::File, os::unix::fs::FileExt, path::Path};

const SECTOR_BYTES: usize = 512;
const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;

fn checked_request_end(sector: u64, data_bytes: usize, disk_sectors: u64) -> Option<u64> {
    if data_bytes == 0 || data_bytes > MAX_REQUEST_BYTES || data_bytes % SECTOR_BYTES != 0 {
        return None;
    }
    let sector_count = u64::try_from(data_bytes / SECTOR_BYTES).ok()?;
    sector
        .checked_add(sector_count)
        .filter(|end| *end <= disk_sectors)
}

pub(crate) struct BlockDevice {
    base: Backing,
    base_bytes: u64,
    sectors: u64,
    overlay: BTreeMap<u64, [u8; SECTOR_BYTES]>,
}

enum Backing {
    Memory(Vec<u8>),
    File(File),
    Writable(File),
}

pub(crate) struct Descriptor {
    pub address: u64,
    pub length: u32,
    pub writable: bool,
}

pub(crate) fn parse_chain(
    memory: &crate::guest::GuestMemory,
    table: u64,
    head: u16,
    count: u16,
) -> Result<Vec<Descriptor>, RelayError> {
    let mut chain = Vec::new();
    let mut index = head;
    let mut seen = std::collections::BTreeSet::new();
    while seen.insert(index) {
        if index >= count || chain.len() >= 32 {
            return Err(RelayError::Failed("virtio descriptor chain invalid".into()));
        }
        let address = table
            .checked_add(index as u64 * 16)
            .ok_or_else(|| RelayError::Failed("virtio descriptor address overflow".into()))?;
        let mut raw = [0; 16];
        memory.read(address, &mut raw)?;
        let flags = u16::from_le_bytes(raw[12..14].try_into().unwrap());
        if flags & !3 != 0 {
            return Err(RelayError::Failed(
                "unsupported virtio descriptor flags".into(),
            ));
        }
        chain.push(Descriptor {
            address: u64::from_le_bytes(raw[..8].try_into().unwrap()),
            length: u32::from_le_bytes(raw[8..12].try_into().unwrap()),
            writable: u16::from_le_bytes(raw[12..14].try_into().unwrap()) & 2 != 0,
        });
        if u16::from_le_bytes(raw[12..14].try_into().unwrap()) & 1 == 0 {
            return Ok(chain);
        }
        index = u16::from_le_bytes(raw[14..16].try_into().unwrap());
    }
    Err(RelayError::Failed("virtio descriptor chain loop".into()))
}

impl BlockDevice {
    pub(crate) fn new(base: Vec<u8>, size_gib: u32) -> Result<Self, RelayError> {
        if size_gib < DiskResizePlan::MIN_GIB {
            return Err(RelayError::Failed(
                "MicroVM disk must be at least 4 GiB".into(),
            ));
        }
        let bytes = (size_gib as u64)
            .checked_mul(GIB)
            .ok_or_else(|| RelayError::Failed("disk capacity overflow".into()))?;
        if base.len() as u64 > bytes {
            return Err(RelayError::Failed(
                "base image exceeds virtual disk capacity".into(),
            ));
        }
        Ok(Self {
            base_bytes: base.len() as u64,
            base: Backing::Memory(base),
            sectors: bytes / SECTOR_BYTES as u64,
            overlay: BTreeMap::new(),
        })
    }
    pub(crate) fn from_file(path: &Path, size_gib: u32) -> Result<Self, RelayError> {
        let file = File::open(path)
            .map_err(|error| RelayError::Failed(format!("cannot open rootfs backing: {error}")))?;
        let base_bytes = file
            .metadata()
            .map_err(|error| RelayError::Failed(format!("cannot inspect rootfs backing: {error}")))?
            .len();
        let bytes = u64::from(size_gib)
            .checked_mul(GIB)
            .ok_or_else(|| RelayError::Failed("disk capacity overflow".into()))?;
        if size_gib < DiskResizePlan::MIN_GIB || base_bytes > bytes {
            return Err(RelayError::Failed(
                "rootfs backing does not fit virtual disk capacity".into(),
            ));
        }
        Ok(Self {
            base: Backing::File(file),
            base_bytes,
            sectors: bytes / SECTOR_BYTES as u64,
            overlay: BTreeMap::new(),
        })
    }
    pub(crate) fn persistent(file: File) -> Result<Self, RelayError> {
        let bytes = file
            .metadata()
            .map_err(|e| RelayError::Failed(e.to_string()))?
            .len();
        if bytes == 0 || bytes % SECTOR_BYTES as u64 != 0 {
            return Err(RelayError::Failed(
                "persistent disk has invalid sector geometry".into(),
            ));
        }
        Ok(Self {
            base: Backing::Writable(file),
            base_bytes: bytes,
            sectors: bytes / SECTOR_BYTES as u64,
            overlay: BTreeMap::new(),
        })
    }
    fn flush(&self) -> Result<(), RelayError> {
        if let Backing::Writable(file) = &self.base {
            file.sync_data()
                .map_err(|e| RelayError::Failed(format!("cannot flush VM disk: {e}")))?;
        }
        Ok(())
    }
    pub(crate) fn read_sector(&self, sector: u64) -> Result<[u8; SECTOR_BYTES], RelayError> {
        if sector >= self.sectors {
            return Err(RelayError::Failed("virtio block read beyond disk".into()));
        }
        if let Some(data) = self.overlay.get(&sector) {
            return Ok(*data);
        }
        let start = sector
            .checked_mul(SECTOR_BYTES as u64)
            .ok_or_else(|| RelayError::Failed("virtio block address overflow".into()))?;
        let mut data = [0; SECTOR_BYTES];
        let available = self
            .base_bytes
            .saturating_sub(start)
            .min(SECTOR_BYTES as u64) as usize;
        if available != 0 {
            match &self.base {
                Backing::Memory(base) => {
                    let start = usize::try_from(start)
                        .map_err(|_| RelayError::Failed("virtio block address overflow".into()))?;
                    data[..available].copy_from_slice(&base[start..start + available]);
                }
                Backing::File(file) | Backing::Writable(file) => {
                    let mut read = 0;
                    while read < available {
                        let count = file
                            .read_at(&mut data[read..available], start + read as u64)
                            .map_err(|error| {
                                RelayError::Failed(format!("cannot read rootfs backing: {error}"))
                            })?;
                        if count == 0 {
                            return Err(RelayError::Failed(
                                "rootfs backing ended before declared size".into(),
                            ));
                        }
                        read += count;
                    }
                }
            }
        }
        Ok(data)
    }
    pub(crate) fn write_sector(
        &mut self,
        sector: u64,
        data: [u8; SECTOR_BYTES],
    ) -> Result<(), RelayError> {
        if sector >= self.sectors {
            return Err(RelayError::Failed("virtio block write beyond disk".into()));
        }
        if let Backing::Writable(file) = &self.base {
            return file
                .write_all_at(&data, sector * SECTOR_BYTES as u64)
                .map_err(|e| RelayError::Failed(format!("cannot write VM disk: {e}")));
        }
        self.overlay.insert(sector, data);
        Ok(())
    }
    pub(crate) fn resize(&mut self, plan: DiskResizePlan) -> Result<(), RelayError> {
        let current = (self.sectors * SECTOR_BYTES as u64 / GIB) as u32;
        DiskResizePlan::from_slider(current, plan.target_gib, DiskResizePlan::MAX_GIB, false)?;
        if plan.current_gib != current || plan.target_gib < current {
            return Err(RelayError::Failed(
                "disk resize plan no longer matches device".into(),
            ));
        }
        let bytes = u64::from(plan.target_gib) * GIB;
        if let Backing::Writable(file) = &self.base {
            file.set_len(bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| RelayError::Failed(format!("cannot grow VM disk: {e}")))?;
            self.base_bytes = bytes;
        }
        self.sectors = bytes / SECTOR_BYTES as u64;
        Ok(())
    }
    pub(crate) fn sectors(&self) -> u64 {
        self.sectors
    }

    /// Process a virtio-blk chain: request header, one or more data segments,
    /// and status. Returns the exact number of device-written bytes.
    /// Segment boundaries need not coincide with disk sectors.
    pub(crate) fn process(
        &mut self,
        memory: &mut crate::guest::GuestMemory,
        chain: &[Descriptor],
    ) -> Result<u32, RelayError> {
        if chain.len() < 2
            || chain[0].writable
            || chain[0].length < 16
            || !chain.last().unwrap().writable
            || chain.last().unwrap().length < 1
        {
            return Err(RelayError::Failed(
                "invalid virtio block descriptor chain".into(),
            ));
        }
        // Validate completion memory before performing any disk or RAM write.
        let mut status = [0];
        memory.read(chain.last().unwrap().address, &mut status)?;
        let mut header = [0; 16];
        memory.read(chain[0].address, &mut header)?;
        let kind = u32::from_le_bytes(header[..4].try_into().unwrap());
        let sector = u64::from_le_bytes(header[8..16].try_into().unwrap());
        if !matches!(kind, 0 | 1 | 4 | 8) {
            // A valid but unsupported operation is a protocol completion,
            // not an interpreter failure. No payload or disk bytes change.
            memory.write(chain.last().unwrap().address, &[2])?;
            return Ok(1);
        }
        let data = &chain[1..chain.len() - 1];
        let data_bytes = data.iter().try_fold(0usize, |total, descriptor| {
            total.checked_add(descriptor.length as usize)
        });
        let Some(data_bytes) = data_bytes else {
            return Err(RelayError::Failed(
                "virtio block request is too large".into(),
            ));
        };
        if kind == 8 {
            // VIRTIO_BLK_T_GET_ID is metadata, not sector data (spec 5.2.6).
            // Relay currently exposes one root disk with this stable identity.
            const ID: &[u8; 20] = b"wawona-relay-rootfs\0";
            if data_bytes != ID.len() || data.iter().any(|descriptor| !descriptor.writable) {
                return Err(RelayError::Failed(
                    "virtio block device ID requires 20 writable bytes".into(),
                ));
            }
            for descriptor in data {
                let mut scratch = [0; 20];
                memory.read(
                    descriptor.address,
                    &mut scratch[..descriptor.length as usize],
                )?;
            }
            let mut offset = 0;
            for descriptor in data {
                let end = offset + descriptor.length as usize;
                memory.write(descriptor.address, &ID[offset..end])?;
                offset = end;
            }
            memory.write(chain.last().unwrap().address, &[0])?;
            return Ok(21);
        }
        if kind == 4 {
            if data_bytes != 0 {
                return Err(RelayError::Failed("flush request contains payload".into()));
            }
            self.flush()?;
            memory.write(chain.last().unwrap().address, &[0])?;
            return Ok(1);
        }
        let Some(end_sector) = checked_request_end(sector, data_bytes, self.sectors) else {
            return Err(RelayError::Failed(
                format!("virtio block request size, alignment, or disk span is invalid: kind={kind} sector={sector} bytes={data_bytes} disk_sectors={}", self.sectors),
            ));
        };
        let sector_count = end_sector - sector;
        match kind {
            0 => {
                if data.iter().any(|descriptor| !descriptor.writable) {
                    return Err(RelayError::Failed(
                        "virtio block read buffer must be writable".into(),
                    ));
                }
                // Validate every guest destination before changing any of it.
                for descriptor in data {
                    let mut scratch = vec![0; descriptor.length as usize];
                    memory.read(descriptor.address, &mut scratch)?;
                }
                let mut bytes = Vec::with_capacity(data_bytes);
                for offset in 0..sector_count {
                    bytes.extend_from_slice(&self.read_sector(sector + offset)?);
                }
                let mut offset = 0;
                for descriptor in data {
                    let end = offset + descriptor.length as usize;
                    memory.write(descriptor.address, &bytes[offset..end])?;
                    offset = end;
                }
            }
            1 => {
                if data.iter().any(|descriptor| descriptor.writable) {
                    return Err(RelayError::Failed(
                        "virtio block write buffer must be readable".into(),
                    ));
                }
                // Gather the entire request before mutating the sparse overlay.
                let mut bytes = Vec::with_capacity(data_bytes);
                for descriptor in data {
                    let start = bytes.len();
                    bytes.resize(start + descriptor.length as usize, 0);
                    memory.read(descriptor.address, &mut bytes[start..])?;
                }
                for (offset, chunk) in bytes.chunks_exact(SECTOR_BYTES).enumerate() {
                    self.write_sector(
                        sector + offset as u64,
                        chunk.try_into().expect("sector-sized chunk"),
                    )?;
                }
            }
            _ => {
                return Err(RelayError::Failed(
                    "virtio block request unsupported".into(),
                ))
            }
        }
        // Write-through completion also covers drivers that decline FLUSH.
        if kind == 1 {
            self.flush()?;
        }
        memory.write(chain.last().unwrap().address, &[0])?;
        Ok(if kind == 0 { data_bytes as u32 + 1 } else { 1 })
    }

    /// Drain queue zero after a guest notification and publish used entries.
    /// This is the device boundary that keeps descriptor parsing, disk writes,
    /// completion ordering, and interrupt delivery inside Relay.
    pub(crate) fn process_notified_queue(
        &mut self,
        transport: &mut crate::virtio_mmio::Transport,
        memory: &mut crate::guest::GuestMemory,
    ) -> Result<usize, RelayError> {
        match transport.take_notification() {
            Some(0) => {}
            Some(_) => return Err(RelayError::Failed("virtio block queue is not zero".into())),
            None => return Ok(0),
        }
        let mut completed = 0;
        while let Some(head) = transport.pop_available(0, memory)? {
            let chain = parse_chain(
                memory,
                transport.descriptor_table(0)?,
                head,
                transport.queue_size(0)?,
            )?;
            let bytes_written = self.process(memory, &chain)?;
            transport.complete(0, memory, head, bytes_written)?;
            completed += 1;
        }
        Ok(completed)
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::{checked_request_end, MAX_REQUEST_BYTES, SECTOR_BYTES};

    #[kani::proof]
    fn accepted_request_span_is_aligned_bounded_and_overflow_free() {
        let sector: u64 = kani::any();
        let data_bytes: usize = kani::any();
        let disk_sectors: u64 = kani::any();

        if let Some(end) = checked_request_end(sector, data_bytes, disk_sectors) {
            assert!(data_bytes != 0);
            assert!(data_bytes <= MAX_REQUEST_BYTES);
            assert_eq!(data_bytes % SECTOR_BYTES, 0);
            assert!(end >= sector);
            assert!(end <= disk_sectors);
            assert_eq!(end - sector, (data_bytes / SECTOR_BYTES) as u64);
        }
    }

    #[kani::proof]
    fn overflowing_request_span_is_rejected() {
        let sector: u64 = kani::any();
        let sector_count: u16 = kani::any();
        kani::assume(sector_count != 0);
        kani::assume(sector.checked_add(u64::from(sector_count)).is_none());
        let data_bytes = usize::from(sector_count) * SECTOR_BYTES;

        assert!(checked_request_end(sector, data_bytes, u64::MAX).is_none());
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::guest::GuestMemory;
    use relay_core::GuestPageSize;

    #[test]
    fn malformed_tables_return_errors_without_panicking() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        assert!(parse_chain(&memory, u64::MAX - 8, 1, 2).is_err());
        let mut raw = [0; 16];
        raw[12..14].copy_from_slice(&4u16.to_le_bytes());
        memory.write(0, &raw).unwrap();
        assert!(parse_chain(&memory, 0, 0, 1).is_err());
    }

    #[test]
    fn invalid_completion_cannot_change_disk() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &1u32.to_le_bytes()).unwrap();
        memory.write(512, &[9; 512]).unwrap();
        let mut disk = BlockDevice::new(vec![7; 512], 4).unwrap();
        let chain = [
            Descriptor {
                address: 0,
                length: 16,
                writable: false,
            },
            Descriptor {
                address: 512,
                length: 512,
                writable: false,
            },
            Descriptor {
                address: 4096,
                length: 1,
                writable: true,
            },
        ];
        assert!(disk.process(&mut memory, &chain).is_err());
        assert_eq!(disk.read_sector(0).unwrap(), [7; 512]);
    }

    #[test]
    fn parsed_request_reads_sector_and_completes_on_both_page_sizes() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let mut memory = GuestMemory::allocate(page, page.bytes() as u64).unwrap();
            for (index, (address, length, flags, next)) in [
                (128u64, 16u32, 1u16, 1u16),
                (512, 512, 3, 2),
                (1024, 1, 2, 0),
            ]
            .into_iter()
            .enumerate()
            {
                let mut raw = [0; 16];
                raw[..8].copy_from_slice(&address.to_le_bytes());
                raw[8..12].copy_from_slice(&length.to_le_bytes());
                raw[12..14].copy_from_slice(&flags.to_le_bytes());
                raw[14..].copy_from_slice(&next.to_le_bytes());
                memory.write(index as u64 * 16, &raw).unwrap();
            }
            memory.write(1024, &[255]).unwrap();
            let chain = parse_chain(&memory, 0, 0, 3).unwrap();
            let mut disk = BlockDevice::new(vec![7; 512], 4).unwrap();
            disk.process(&mut memory, &chain).unwrap();
            let mut data = [0; 512];
            memory.read(512, &mut data).unwrap();
            assert_eq!(data, [7; 512]);
            let mut status = [255];
            memory.read(1024, &mut status).unwrap();
            assert_eq!(status, [0]);
        }
    }

    #[test]
    fn scatter_gather_request_crosses_descriptor_and_sector_boundaries() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &0u32.to_le_bytes()).unwrap();
        memory.write(8, &0u64.to_le_bytes()).unwrap();
        memory.write(0xd00, &[255]).unwrap();
        let chain = [
            Descriptor {
                address: 0,
                length: 16,
                writable: false,
            },
            Descriptor {
                address: 0x400,
                length: 256,
                writable: true,
            },
            Descriptor {
                address: 0x800,
                length: 768,
                writable: true,
            },
            Descriptor {
                address: 0xd00,
                length: 1,
                writable: true,
            },
        ];
        let mut base = vec![7; 1024];
        base[512..].fill(9);
        let mut disk = BlockDevice::new(base, 4).unwrap();
        disk.process(&mut memory, &chain).unwrap();

        let mut first = [0; 256];
        let mut rest = [0; 768];
        let mut status = [255];
        memory.read(0x400, &mut first).unwrap();
        memory.read(0x800, &mut rest).unwrap();
        memory.read(0xd00, &mut status).unwrap();
        assert_eq!(first, [7; 256]);
        assert_eq!(&rest[..256], &[7; 256]);
        assert_eq!(&rest[256..], &[9; 512]);
        assert_eq!(status, [0]);
    }

    #[test]
    fn mmio_notification_drains_block_queue_and_raises_interrupt() {
        let mut memory = GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        for (index, (address, length, flags, next)) in [
            (0x600u64, 16u32, 1u16, 1u16),
            (0x700, 512, 3, 2),
            (0x900, 1, 2, 0),
        ]
        .into_iter()
        .enumerate()
        {
            let mut raw = [0; 16];
            raw[..8].copy_from_slice(&address.to_le_bytes());
            raw[8..12].copy_from_slice(&length.to_le_bytes());
            raw[12..14].copy_from_slice(&flags.to_le_bytes());
            raw[14..].copy_from_slice(&next.to_le_bytes());
            memory.write(0x100 + index as u64 * 16, &raw).unwrap();
        }
        memory.write(0x202, &1u16.to_le_bytes()).unwrap();
        memory.write(0x204, &0u16.to_le_bytes()).unwrap();
        memory.write(0x900, &[255]).unwrap();

        let mut transport = crate::virtio_mmio::Transport::new(2, 1 << 32, 8, 1);
        transport.write(0x024, 1).unwrap();
        transport.write(0x020, 1).unwrap();
        transport.write(0x038, 8).unwrap();
        transport.write(0x080, 0x100).unwrap();
        transport.write(0x090, 0x200).unwrap();
        transport.write(0x0a0, 0x300).unwrap();
        transport.write(0x044, 1).unwrap();
        for status in [1, 3, 11, 15] {
            transport.write(0x070, status).unwrap();
        }
        transport.write(0x050, 0).unwrap();

        let mut disk = BlockDevice::new(vec![7; 512], 4).unwrap();
        assert_eq!(
            disk.process_notified_queue(&mut transport, &mut memory)
                .unwrap(),
            1
        );
        let mut data = [0; 512];
        memory.read(0x700, &mut data).unwrap();
        assert_eq!(data, [7; 512]);
        let mut used_index = [0; 2];
        memory.read(0x302, &mut used_index).unwrap();
        assert_eq!(u16::from_le_bytes(used_index), 1);
        assert_eq!(transport.read(0x060).unwrap(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::GuestPageSize;
    use std::io::Write;
    #[test]
    fn overlay_preserves_base_and_grows_sparsely() {
        let mut disk = BlockDevice::new(vec![7; SECTOR_BYTES], 4).unwrap();
        assert_eq!(disk.read_sector(0).unwrap()[0], 7);
        disk.write_sector(1, [9; SECTOR_BYTES]).unwrap();
        assert_eq!(disk.read_sector(1).unwrap()[0], 9);
        disk.resize(DiskResizePlan::from_slider(4, 8, 16, false).unwrap())
            .unwrap();
        assert_eq!(disk.read_sector(9_000_000).unwrap()[0], 0);
    }
    #[test]
    fn file_backing_reads_without_loading_rootfs_into_ram() {
        let path = std::env::temp_dir().join(format!("relay-block-{}", std::process::id()));
        File::create(&path)
            .unwrap()
            .write_all(&[7; SECTOR_BYTES])
            .unwrap();
        let disk = BlockDevice::from_file(&path, 4).unwrap();
        assert_eq!(disk.read_sector(0).unwrap(), [7; SECTOR_BYTES]);
        assert_eq!(disk.read_sector(1).unwrap(), [0; SECTOR_BYTES]);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn persistent_queue_writes_flush_and_growth_survive_reopen() {
        let path =
            std::env::temp_dir().join(format!("relay-persistent-block-{}", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.set_len(4 * GIB).unwrap();
        let mut disk = BlockDevice::persistent(file).unwrap();
        let mut memory =
            crate::guest::GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        memory.write(0, &1u32.to_le_bytes()).unwrap();
        memory.write(8, &1u64.to_le_bytes()).unwrap();
        memory.write(512, &[0x69; 512]).unwrap();
        memory.write(128, &[255]).unwrap();
        let chain = [
            Descriptor {
                address: 0,
                length: 16,
                writable: false,
            },
            Descriptor {
                address: 512,
                length: 512,
                writable: false,
            },
            Descriptor {
                address: 128,
                length: 1,
                writable: true,
            },
        ];
        assert_eq!(disk.process(&mut memory, &chain).unwrap(), 1);
        let mut status = [255];
        memory.read(128, &mut status).unwrap();
        assert_eq!(status, [0]);
        memory.write(0, &4u32.to_le_bytes()).unwrap();
        memory.write(128, &[255]).unwrap();
        assert!(disk.process(&mut memory, &chain).is_err());
        memory.read(128, &mut status).unwrap();
        assert_eq!(status, [255]);
        let [header, _, status] = chain;
        assert_eq!(disk.process(&mut memory, &[header, status]).unwrap(), 1);
        disk.resize(DiskResizePlan::from_slider(4, 8, 64, false).unwrap())
            .unwrap();
        disk.write_sector(7 * GIB / 512, [0x42; 512]).unwrap();
        disk.flush().unwrap();
        assert!(disk
            .resize(DiskResizePlan {
                current_gib: 8,
                target_gib: 65
            })
            .is_err());
        drop(disk);
        let disk = BlockDevice::persistent(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(disk.sectors(), 8 * GIB / 512);
        assert_eq!(disk.read_sector(1).unwrap(), [0x69; 512]);
        assert_eq!(disk.read_sector(7 * GIB / 512).unwrap(), [0x42; 512]);
        drop(disk);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn parses_guest_chain_and_rejects_loop() {
        let mut mem = crate::guest::GuestMemory::allocate(GuestPageSize::FOUR_KIB, 4096).unwrap();
        let mut first = [0; 16];
        first[..8].copy_from_slice(&0x100u64.to_le_bytes());
        first[8..12].copy_from_slice(&16u32.to_le_bytes());
        first[12..14].copy_from_slice(&1u16.to_le_bytes());
        first[14..16].copy_from_slice(&1u16.to_le_bytes());
        let mut second = first;
        second[12..14].copy_from_slice(&0u16.to_le_bytes());
        mem.write(0, &first).unwrap();
        mem.write(16, &second).unwrap();
        assert_eq!(parse_chain(&mem, 0, 0, 2).unwrap().len(), 2);
        mem.write(16, &first).unwrap();
        assert!(parse_chain(&mem, 0, 0, 2).is_err());
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use crate::guest::GuestMemory;
    use relay_core::{GuestPageSize, HostPageSize};

    fn request(page: GuestPageSize, kind: u32) -> (GuestMemory, Vec<Descriptor>) {
        let mut memory =
            GuestMemory::allocate_on_host(page, HostPageSize::SIXTEEN_KIB, page.bytes() as u64)
                .unwrap();
        memory.write(0, &kind.to_le_bytes()).unwrap();
        memory.write(128, &[255; 8]).unwrap();
        memory.write(512, &[0xa5; 24]).unwrap();
        memory.write(1024, &[0xa5; 24]).unwrap();
        let chain = vec![
            Descriptor {
                address: 0,
                length: 16,
                writable: false,
            },
            Descriptor {
                address: 512,
                length: 7,
                writable: true,
            },
            Descriptor {
                address: 1024,
                length: 13,
                writable: true,
            },
            Descriptor {
                address: 128,
                length: 8,
                writable: true,
            },
        ];
        (memory, chain)
    }

    #[test]
    fn device_id_is_twenty_ascii_bytes_across_segments_on_both_page_sizes() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let (mut memory, chain) = request(page, 8);
            let mut disk = BlockDevice::new(vec![7; 512], 4).unwrap();
            disk.process(&mut memory, &chain).unwrap();
            let mut id = [0; 20];
            memory.read(512, &mut id[..7]).unwrap();
            memory.read(1024, &mut id[7..]).unwrap();
            assert_eq!(&id, b"wawona-relay-rootfs\0");
            let mut status = [0; 8];
            memory.read(128, &mut status).unwrap();
            assert_eq!(status, [0, 255, 255, 255, 255, 255, 255, 255]);
            let mut guard = [0];
            memory.read(519, &mut guard).unwrap();
            assert_eq!(guard, [0xa5]);
            memory.read(1037, &mut guard).unwrap();
            assert_eq!(guard, [0xa5]);
            assert_eq!(disk.read_sector(0).unwrap(), [7; 512]);
        }
    }

    #[test]
    fn malformed_device_id_requests_do_not_partially_write() {
        for malformed in 0..4 {
            let (mut memory, mut chain) = request(GuestPageSize::FOUR_KIB, 8);
            match malformed {
                0 => chain[2].length = 12,
                1 => chain[2].length = 14,
                2 => chain[2].writable = false,
                _ => chain[2].address = 4090,
            }
            let mut disk = BlockDevice::new(vec![7; 512], 4).unwrap();
            assert!(disk.process(&mut memory, &chain).is_err());
            let mut unchanged = [0; 24];
            memory.read(512, &mut unchanged).unwrap();
            assert_eq!(unchanged, [0xa5; 24]);
            let mut status = [0; 8];
            memory.read(128, &mut status).unwrap();
            assert_eq!(status, [255; 8]);
        }
    }

    #[test]
    fn unsupported_request_completes_with_protocol_status() {
        let (mut memory, chain) = request(GuestPageSize::FOUR_KIB, 0xffff);
        let mut disk = BlockDevice::new(vec![7; 512], 4).unwrap();
        disk.process(&mut memory, &chain).unwrap();
        let mut status = [0];
        memory.read(128, &mut status).unwrap();
        assert_eq!(status, [2]);
        let mut unchanged = [0; 24];
        memory.read(512, &mut unchanged).unwrap();
        assert_eq!(unchanged, [0xa5; 24]);
        assert_eq!(disk.read_sector(0).unwrap(), [7; 512]);
    }
    #[test]
    fn metadata_used_length_counts_written_bytes_not_buffer_capacity() {
        for (kind, expected) in [(8, 21u32), (0xffff, 1)] {
            let (mut memory, chain) = request(GuestPageSize::FOUR_KIB, kind);
            for (index, descriptor) in chain.iter().enumerate() {
                let mut raw = [0u8; 16];
                raw[..8].copy_from_slice(&descriptor.address.to_le_bytes());
                raw[8..12].copy_from_slice(&descriptor.length.to_le_bytes());
                let flags: u16 =
                    u16::from(descriptor.writable) * 2 | u16::from(index + 1 < chain.len());
                raw[12..14].copy_from_slice(&flags.to_le_bytes());
                raw[14..].copy_from_slice(&((index + 1) as u16).to_le_bytes());
                memory.write(0x100 + index as u64 * 16, &raw).unwrap();
            }
            memory.write(0x802, &1u16.to_le_bytes()).unwrap();
            let mut transport = crate::virtio_mmio::Transport::new(2, 1 << 32, 8, 1);
            for (register, value) in [
                (0x024, 1),
                (0x020, 1),
                (0x038, 8),
                (0x080, 0x100),
                (0x090, 0x800),
                (0x0a0, 0x900),
                (0x044, 1),
            ] {
                transport.write(register, value).unwrap();
            }
            for status in [1, 3, 11, 15] {
                transport.write(0x070, status).unwrap();
            }
            transport.write(0x050, 0).unwrap();
            let mut disk = BlockDevice::new(vec![7; 512], 4).unwrap();
            assert_eq!(
                disk.process_notified_queue(&mut transport, &mut memory)
                    .unwrap(),
                1
            );
            let mut used = [0; 12];
            memory.read(0x900, &mut used).unwrap();
            assert_eq!(u16::from_le_bytes(used[2..4].try_into().unwrap()), 1);
            assert_eq!(
                u32::from_le_bytes(used[8..12].try_into().unwrap()),
                expected
            );
            assert_eq!(transport.read(0x060).unwrap(), 1);
        }
    }
}

//! Sparse virtio-block backend: immutable NixOS base plus writable overlay.

#![allow(dead_code)] // The static CPU bus will expose this device next.

use relay_core::{DiskResizePlan, RelayError};
use std::collections::BTreeMap;

const SECTOR_BYTES: usize = 512;
const GIB: u64 = 1024 * 1024 * 1024;

pub(crate) struct BlockDevice {
    base: Vec<u8>,
    sectors: u64,
    overlay: BTreeMap<u64, [u8; SECTOR_BYTES]>,
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
            base,
            sectors: bytes / SECTOR_BYTES as u64,
            overlay: BTreeMap::new(),
        })
    }
    pub(crate) fn read_sector(&self, sector: u64) -> Result<[u8; SECTOR_BYTES], RelayError> {
        if sector >= self.sectors {
            return Err(RelayError::Failed("virtio block read beyond disk".into()));
        }
        if let Some(data) = self.overlay.get(&sector) {
            return Ok(*data);
        }
        let start = usize::try_from(sector)
            .ok()
            .and_then(|sector| sector.checked_mul(SECTOR_BYTES))
            .ok_or_else(|| RelayError::Failed("virtio block address overflow".into()))?;
        let mut data = [0; SECTOR_BYTES];
        if start < self.base.len() {
            let end = (start + SECTOR_BYTES).min(self.base.len());
            data[..end - start].copy_from_slice(&self.base[start..end]);
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
        self.overlay.insert(sector, data);
        Ok(())
    }
    pub(crate) fn resize(&mut self, plan: DiskResizePlan) -> Result<(), RelayError> {
        let current = (self.sectors * SECTOR_BYTES as u64 / GIB) as u32;
        if plan.current_gib != current || plan.target_gib < current {
            return Err(RelayError::Failed(
                "disk resize plan no longer matches device".into(),
            ));
        }
        self.sectors = plan.target_gib as u64 * GIB / SECTOR_BYTES as u64;
        Ok(())
    }

    /// Process a three-descriptor virtio-blk chain: request header, data, status.
    pub(crate) fn process(
        &mut self,
        memory: &mut crate::guest::GuestMemory,
        chain: &[Descriptor],
    ) -> Result<(), RelayError> {
        if chain.len() != 3
            || chain[0].writable
            || chain[0].length < 16
            || !chain[2].writable
            || chain[2].length < 1
        {
            return Err(RelayError::Failed(
                "invalid virtio block descriptor chain".into(),
            ));
        }
        // Validate completion memory before performing any disk or RAM write.
        let mut status = [0];
        memory.read(chain[2].address, &mut status)?;
        let mut header = [0; 16];
        memory.read(chain[0].address, &mut header)?;
        let kind = u32::from_le_bytes(header[..4].try_into().unwrap());
        let sector = u64::from_le_bytes(header[8..16].try_into().unwrap());
        if chain[1].length != SECTOR_BYTES as u32 {
            return Err(RelayError::Failed(
                "virtio block currently requires one sector descriptor".into(),
            ));
        }
        match kind {
            0 => {
                if !chain[1].writable {
                    return Err(RelayError::Failed(
                        "virtio block read buffer must be writable".into(),
                    ));
                }
                memory.write(chain[1].address, &self.read_sector(sector)?)?;
            }
            1 => {
                if chain[1].writable {
                    return Err(RelayError::Failed(
                        "virtio block write buffer must be readable".into(),
                    ));
                }
                let mut data = [0; SECTOR_BYTES];
                memory.read(chain[1].address, &mut data)?;
                self.write_sector(sector, data)?;
            }
            _ => {
                return Err(RelayError::Failed(
                    "virtio block request unsupported".into(),
                ))
            }
        }
        memory.write(chain[2].address, &[0])?;
        Ok(())
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
        while let Some(head) = transport.pop_available(memory)? {
            let chain = parse_chain(
                memory,
                transport.descriptor_table(),
                head,
                transport.queue_size(),
            )?;
            self.process(memory, &chain)?;
            let bytes_written = chain
                .iter()
                .filter(|descriptor| descriptor.writable)
                .try_fold(0u32, |total, descriptor| {
                    total.checked_add(descriptor.length)
                })
                .ok_or_else(|| {
                    RelayError::Failed("virtio block completion length overflow".into())
                })?;
            transport.complete(memory, head, bytes_written)?;
            completed += 1;
        }
        Ok(completed)
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

        let mut transport = crate::virtio_mmio::Transport::new(2, 1 << 32, 8);
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

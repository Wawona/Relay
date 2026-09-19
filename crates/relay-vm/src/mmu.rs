//! Page-size-aware stage-1 translation for Relay's static AArch64 CPU.

#![allow(dead_code)] // Wired into the static CPU when exception state lands.

use relay_core::{GuestPageSize, RelayError};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Access {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}
impl Access {
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
}

#[derive(Debug, Clone, Copy)]
struct Mapping {
    physical: u64,
    access: Access,
}

pub(crate) struct Mmu {
    page_size: GuestPageSize,
    pages: BTreeMap<u64, Mapping>,
}

impl Mmu {
    pub fn new(page_size: GuestPageSize) -> Self {
        Self {
            page_size,
            pages: BTreeMap::new(),
        }
    }
    pub fn map(
        &mut self,
        virtual_address: u64,
        physical_address: u64,
        bytes: u64,
        access: Access,
    ) -> Result<(), RelayError> {
        if bytes == 0
            || !self.page_size.is_aligned(virtual_address)
            || !self.page_size.is_aligned(physical_address)
            || bytes % self.page_size.0 as u64 != 0
        {
            return Err(RelayError::Failed(
                "MMU mappings must be non-empty and page aligned".into(),
            ));
        }
        for offset in (0..bytes).step_by(self.page_size.bytes()) {
            let va = virtual_address
                .checked_add(offset)
                .ok_or_else(|| RelayError::Failed("MMU virtual mapping overflow".into()))?;
            let pa = physical_address
                .checked_add(offset)
                .ok_or_else(|| RelayError::Failed("MMU physical mapping overflow".into()))?;
            if self
                .pages
                .insert(
                    va,
                    Mapping {
                        physical: pa,
                        access,
                    },
                )
                .is_some()
            {
                return Err(RelayError::Failed(
                    "MMU mapping overlaps an existing page".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn translate(&self, virtual_address: u64, access: Access) -> Result<u64, RelayError> {
        let page = virtual_address & !(self.page_size.0 as u64 - 1);
        let offset = virtual_address - page;
        let mapping = self
            .pages
            .get(&page)
            .ok_or_else(|| RelayError::Failed("MMU translation fault".into()))?;
        if (access.read && !mapping.access.read)
            || (access.write && !mapping.access.write)
            || (access.execute && !mapping.access.execute)
        {
            return Err(RelayError::Failed("MMU permission fault".into()));
        }
        Ok(mapping.physical + offset)
    }
}

/// Translate an EL1 stage-1 virtual address through guest-owned page tables.
/// Both supported Linux granules use four levels; block descriptors are valid
/// before the final level and page descriptors are required at the final one.
pub(crate) fn walk_stage1(
    memory: &crate::guest::GuestMemory,
    virtual_address: u64,
    ttbr0_el1: u64,
    page_size: GuestPageSize,
) -> Result<u64, RelayError> {
    const PHYSICAL_ADDRESS_MASK: u64 = (1u64 << 48) - 1;
    let (shifts, entries) = match page_size.0 {
        4096 => ([39u32, 30, 21, 12], 512u64),
        16384 => ([47u32, 36, 25, 14], 2048u64),
        _ => return Err(RelayError::Failed("unsupported stage-1 granule".into())),
    };
    let mut table = ttbr0_el1 & PHYSICAL_ADDRESS_MASK & !(page_size.0 as u64 - 1);
    for (level, shift) in shifts.into_iter().enumerate() {
        let index = (virtual_address >> shift) & (entries - 1);
        let address = table
            .checked_add(index * 8)
            .ok_or_else(|| RelayError::Failed("MMU table address overflow".into()))?;
        let mut raw = [0; 8];
        memory.read(address, &mut raw)?;
        let descriptor = u64::from_le_bytes(raw);
        if descriptor & 1 == 0 {
            return Err(RelayError::Failed(format!(
                "stage-1 translation fault va={virtual_address:#x}"
            )));
        }
        if level < 3 && descriptor & 2 != 0 {
            table = descriptor & PHYSICAL_ADDRESS_MASK & !(page_size.0 as u64 - 1);
            continue;
        }
        let base_mask = !((1u64 << shift) - 1);
        return Ok(
            (descriptor & PHYSICAL_ADDRESS_MASK & base_mask) | (virtual_address & !base_mask)
        );
    }
    Err(RelayError::Failed(
        "stage-1 page-table walk did not terminate".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_and_faults_by_permission() {
        let mut mmu = Mmu::new(GuestPageSize::SIXTEEN_KIB);
        mmu.map(0x4000, 0x8000, 16384, Access::RX).unwrap();
        assert_eq!(mmu.translate(0x4004, Access::RX).unwrap(), 0x8004);
        assert!(mmu.translate(0x4004, Access::RW).is_err());
        assert!(mmu.translate(0x9000, Access::RX).is_err());
    }

    #[test]
    fn walks_4k_and_16k_final_page_descriptors() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let bytes = page.bytes() as u64 * 8;
            let mut memory = crate::guest::GuestMemory::allocate(page, bytes).unwrap();
            let shifts = if page.0 == 4096 {
                [39, 30, 21, 12]
            } else {
                [47, 36, 25, 14]
            };
            let entries = if page.0 == 4096 { 512 } else { 2048 };
            let va = page.0 as u64 + 0x44;
            for (level, shift) in shifts.into_iter().enumerate() {
                let table = level as u64 * page.0 as u64;
                let index = (va >> shift) & (entries - 1);
                let descriptor = (if level == 3 {
                    0x400000 | 3
                } else {
                    (level as u64 + 1) * page.0 as u64 | 3
                }) | (1u64 << 60);
                memory
                    .write(table + index * 8, &descriptor.to_le_bytes())
                    .unwrap();
            }
            assert_eq!(walk_stage1(&memory, va, 0, page).unwrap(), 0x400044);
        }
    }
}

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
}

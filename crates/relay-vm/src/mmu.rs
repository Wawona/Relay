//! Page-size-aware stage-1 translation for Relay's static AArch64 CPU.

#![allow(dead_code)] // Wired into the static CPU when exception state lands.

use relay_core::{GuestPageSize, RelayError};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(crate) enum AccessType {
    Read,
    Write,
    Execute { wxn: bool },
}

/// Architectural walk faults carry their syndrome, never a parsed error string.
/// Invalid runtime configuration remains an error rather than a guest fault.
#[derive(Debug)]
pub(crate) struct WalkError {
    pub status: Option<u8>,
    pub error: RelayError,
}

impl From<RelayError> for WalkError {
    fn from(error: RelayError) -> Self {
        Self {
            status: None,
            error,
        }
    }
}

/// Baseline EL1&0 stage-1 permissions. HPDS/permission indirection are not
/// advertised. Table restrictions accumulate and cannot be relaxed by a leaf.
fn permits(descriptor: u64, tables: u64, access: AccessType, current_el: u8) -> bool {
    let user = descriptor & (1 << 6) != 0 && tables & (1 << 61) == 0;
    let writable = descriptor & (1 << 7) == 0 && tables & (1 << 62) == 0;
    if current_el == 0 && !user {
        return false;
    }
    match access {
        AccessType::Read => true,
        AccessType::Write => writable,
        AccessType::Execute { wxn } => {
            let xn = if current_el == 0 {
                descriptor & (1 << 54) != 0 || tables & (1 << 60) != 0
            } else {
                descriptor & (1 << 53) != 0 || tables & (1 << 59) != 0 || (user && writable)
            };
            !xn && !(wxn && writable)
        }
    }
}

#[cfg(kani)]
#[kani::proof]
fn executable_mappings_obey_privilege_and_execute_never() {
    let descriptor: u64 = kani::any();
    let tables: u64 = kani::any();
    let el = if kani::any::<bool>() { 0 } else { 1 };
    let wxn: bool = kani::any();
    let user = descriptor & (1 << 6) != 0 && tables & (1 << 61) == 0;
    let writable = descriptor & (1 << 7) == 0 && tables & (1 << 62) == 0;
    if permits(descriptor, tables, AccessType::Execute { wxn }, el) {
        assert!(el != 0 || user);
        assert!(el != 1 || !(user && writable));
        assert!(!wxn || !writable);
        if el == 0 {
            assert_eq!(descriptor & (1 << 54), 0);
            assert_eq!(tables & (1 << 60), 0);
        } else {
            assert_eq!(descriptor & (1 << 53), 0);
            assert_eq!(tables & (1 << 59), 0);
        }
    }
    // Both privilege domains have executable witnesses; the predicate is
    // not allowed to satisfy the safety properties by rejecting everything.
    assert!(permits(3 << 6, 0, AccessType::Execute { wxn: true }, el));
}

#[cfg(kani)]
#[kani::proof]
fn data_permissions_ignore_execute_never_and_respect_tables() {
    let descriptor: u64 = kani::any();
    let tables: u64 = kani::any();
    let el = if kani::any::<bool>() { 0 } else { 1 };
    let access = if kani::any::<bool>() {
        AccessType::Read
    } else {
        AccessType::Write
    };
    assert_eq!(
        permits(descriptor, tables, access, el),
        permits(descriptor ^ (3 << 53), tables ^ (3 << 59), access, el)
    );
    assert!(!permits(
        descriptor,
        tables | (1 << 62),
        AccessType::Write,
        el
    ));
    assert!(!permits(descriptor, tables | (1 << 61), access, 0));
}

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
/// This convenience wrapper uses 48 VA bits. Production CPU walks derive
/// their address size and initial level from the selected TCR_EL1.TxSZ.
pub(crate) fn walk_stage1(
    memory: &crate::guest::GuestMemory,
    virtual_address: u64,
    ttbr0_el1: u64,
    page_size: GuestPageSize,
) -> Result<u64, RelayError> {
    walk_stage1_access(memory, virtual_address, ttbr0_el1, page_size, false, 1)
}

pub(crate) fn walk_stage1_access(
    memory: &crate::guest::GuestMemory,
    virtual_address: u64,
    ttbr0_el1: u64,
    page_size: GuestPageSize,
    write: bool,
    current_el: u8,
) -> Result<u64, RelayError> {
    walk_stage1_access_with_va_bits(
        memory,
        virtual_address,
        ttbr0_el1,
        page_size,
        write,
        current_el,
        48,
    )
}

pub(crate) fn walk_stage1_access_with_va_bits(
    memory: &crate::guest::GuestMemory,
    virtual_address: u64,
    ttbr: u64,
    page_size: GuestPageSize,
    write: bool,
    current_el: u8,
    virtual_bits: u32,
) -> Result<u64, RelayError> {
    walk_stage1_with_access(
        memory,
        virtual_address,
        ttbr,
        page_size,
        if write {
            AccessType::Write
        } else {
            AccessType::Read
        },
        current_el,
        virtual_bits,
    )
    .map_err(|fault| fault.error)
}

pub(crate) fn walk_stage1_with_access(
    memory: &crate::guest::GuestMemory,
    virtual_address: u64,
    ttbr: u64,
    page_size: GuestPageSize,
    access: AccessType,
    current_el: u8,
    virtual_bits: u32,
) -> Result<u64, WalkError> {
    const PHYSICAL_ADDRESS_MASK: u64 = (1u64 << 48) - 1;
    if !(25..=48).contains(&virtual_bits) {
        return Err(RelayError::Failed("unsupported stage-1 virtual address size".into()).into());
    }
    let virtual_mask = (1u64 << virtual_bits) - 1;
    let (shifts, entries) = match page_size.0 {
        4096 => ([39u32, 30, 21, 12], 512u64),
        16384 => ([47u32, 36, 25, 14], 2048u64),
        _ => return Err(RelayError::Failed("unsupported stage-1 granule".into()).into()),
    };
    let mut table = ttbr & PHYSICAL_ADDRESS_MASK & !(page_size.0 as u64 - 1);
    let mut restrictions = 0;
    for (level, shift) in shifts.into_iter().enumerate() {
        // TCR.TxSZ selects the first table independently for each address
        // half. Ignore canonical sign-extension bits in the initial index.
        if shift >= virtual_bits {
            continue;
        }
        let index = ((virtual_address & virtual_mask) >> shift) & (entries - 1);
        let address = table
            .checked_add(index * 8)
            .ok_or_else(|| RelayError::Failed("MMU table address overflow".into()))?;
        let mut raw = [0; 8];
        memory.read(address, &mut raw).map_err(|error| WalkError {
            status: Some(0x14 | level as u8),
            error,
        })?;
        let descriptor = u64::from_le_bytes(raw);
        // Level 3 accepts page descriptors (0b11), never block (0b01).
        if descriptor & 1 == 0 || (level == 3 && descriptor & 2 == 0) {
            return Err(WalkError { status: Some(0x04 | level as u8), error: RelayError::Failed(format!(
                "stage-1 translation fault va={virtual_address:#x} level={level} table={table:#x} descriptor-address={address:#x} descriptor={descriptor:#x} ttbr={ttbr:#x}"
            )) });
        }
        if level < 3 && descriptor & 2 != 0 {
            restrictions |= descriptor;
            table = descriptor & PHYSICAL_ADDRESS_MASK & !(page_size.0 as u64 - 1);
            continue;
        }
        let base_mask = !((1u64 << shift) - 1);
        // HAFDBS is not advertised: software owns Access Flag updates.
        if descriptor & (1 << 10) == 0 {
            return Err(WalkError {
                status: Some(0x08 | level as u8),
                error: RelayError::Failed(format!(
                    "stage-1 access flag fault va={virtual_address:#x} level={level}"
                )),
            });
        }
        if !permits(descriptor, restrictions, access, current_el) {
            return Err(WalkError {
                status: Some(0x0c | level as u8),
                error: RelayError::Failed(format!(
                    "stage-1 permission fault va={virtual_address:#x} level={level}: access denied"
                )),
            });
        }
        return Ok(
            (descriptor & PHYSICAL_ADDRESS_MASK & base_mask) | (virtual_address & !base_mask)
        );
    }
    Err(RelayError::Failed("stage-1 page-table walk did not terminate".into()).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn access_flag_fault_precedes_permissions_and_preserves_descriptor() {
        // Software AF management: Arm 102376 section 11.1. Relay advertises
        // no HAFDBS, so every access to an AF=0 leaf must fault, not update it.
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let size = u64::from(page.0);
            for level in 1..=3u8 {
                if page == GuestPageSize::SIXTEEN_KIB && level == 1 {
                    continue;
                }
                let mut memory = crate::guest::GuestMemory::allocate_on_host(
                    page,
                    relay_core::HostPageSize::SIXTEEN_KIB,
                    size * 4,
                )
                .unwrap();
                for parent in 0..level {
                    memory
                        .write(
                            u64::from(parent) * size,
                            &((u64::from(parent) + 1) * size | 3).to_le_bytes(),
                        )
                        .unwrap();
                }
                let entry = u64::from(level) * size;
                let descriptor = if level == 3 { 3u64 } else { 1 } | (3 << 6);
                for access in [
                    AccessType::Read,
                    AccessType::Write,
                    AccessType::Execute { wxn: false },
                ] {
                    for el in [0, 1] {
                        memory.write(entry, &descriptor.to_le_bytes()).unwrap();
                        let fault = walk_stage1_with_access(&memory, 0, 0, page, access, el, 48)
                            .unwrap_err();
                        assert_eq!(fault.status, Some(0x08 | level));
                        let mut actual = [0; 8];
                        memory.read(entry, &mut actual).unwrap();
                        assert_eq!(actual, descriptor.to_le_bytes());
                    }
                }
                memory
                    .write(entry, &(descriptor | (1 << 10)).to_le_bytes())
                    .unwrap();
                assert_eq!(
                    walk_stage1_with_access(&memory, 0, 0, page, AccessType::Read, 0, 48).unwrap(),
                    0
                );
            }
        }
    }
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
            let mut memory = crate::guest::GuestMemory::allocate_on_host(
                page,
                relay_core::HostPageSize::SIXTEEN_KIB,
                bytes,
            )
            .unwrap();
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
                    0x400000 | 3 | (1 << 10)
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

    #[test]
    fn unsupported_virtual_address_sizes_fail_before_walking() {
        let page = GuestPageSize::FOUR_KIB;
        let memory = crate::guest::GuestMemory::allocate_on_host(
            page,
            relay_core::HostPageSize::SIXTEEN_KIB,
            4096,
        )
        .unwrap();
        for bits in [0, 1, 24, 49, 52, 64, u32::MAX] {
            let error =
                walk_stage1_access_with_va_bits(&memory, 0, 0, page, false, 1, bits).unwrap_err();
            assert!(error
                .to_string()
                .contains("unsupported stage-1 virtual address size"));
        }
    }

    #[test]
    fn stage1_high_half_uses_only_48_virtual_address_bits() {
        for page in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            let size = page.0 as u64;
            let mut memory = crate::guest::GuestMemory::allocate_on_host(
                page,
                relay_core::HostPageSize::SIXTEEN_KIB,
                8 * size,
            )
            .unwrap();
            let va = 0xffff_8000_0000_0123u64;
            let shifts = if size == 4096 {
                [39, 30, 21, 12]
            } else {
                [47, 36, 25, 14]
            };
            for (level, shift) in shifts.into_iter().enumerate() {
                // A 48-bit VA has only one level-zero index bit for 16 KiB
                // tables. Upper canonical ones are not table index bits.
                let index = ((va & ((1u64 << 48) - 1)) >> shift) & (size / 8 - 1);
                let next = if level == 3 {
                    7 * size
                } else {
                    (level as u64 + 1) * size
                };
                memory
                    .write(
                        level as u64 * size + index * 8,
                        &(next | 3 | (1 << 10)).to_le_bytes(),
                    )
                    .unwrap();
            }
            assert_eq!(walk_stage1(&memory, va, 0, page).unwrap(), 7 * size + 0x123);
        }
    }

    #[test]
    fn stage1_enforces_el0_and_write_permissions() {
        let page = GuestPageSize::FOUR_KIB;
        let mut memory = crate::guest::GuestMemory::allocate_on_host(
            page,
            relay_core::HostPageSize::SIXTEEN_KIB,
            8 * page.bytes() as u64,
        )
        .unwrap();
        let va = 0x1234u64;
        let shifts = [39, 30, 21, 12];

        for (level, shift) in shifts.into_iter().enumerate() {
            let table = level as u64 * page.0 as u64;
            let index = (va >> shift) & 511;
            let descriptor = if level == 3 {
                0x400000 | 3 | (1 << 10) | (3 << 6) // AP=11: EL0-readable, read-only.
            } else {
                (level as u64 + 1) * page.0 as u64 | 3
            };
            memory
                .write(table + index * 8, &descriptor.to_le_bytes())
                .unwrap();
        }

        assert_eq!(
            walk_stage1_access(&memory, va, 0, page, false, 0).unwrap(),
            0x400234
        );
        assert!(walk_stage1_access(&memory, va, 0, page, true, 0).is_err());

        let final_entry = 3 * page.0 as u64 + ((va >> 12) & 511) * 8;
        memory
            .write(final_entry, &(0x400000u64 | 3 | (1 << 10)).to_le_bytes())
            .unwrap();
        assert!(walk_stage1_access(&memory, va, 0, page, false, 0).is_err());
        assert_eq!(
            walk_stage1_access(&memory, va, 0, page, true, 1).unwrap(),
            0x400234
        );
    }
}

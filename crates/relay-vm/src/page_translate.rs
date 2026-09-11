//! Host↔guest page geometry for one Relay StaticCpu.
//!
//! Guest Linux is still built as a real 4 KiB or 16 KiB kernel Image. What this
//! module removes is a second VM *product* per page size: the same backend maps
//! either guest geometry onto the host process page size (Asahi-class intent).

use relay_core::{GuestPageSize, HostPageSize, RelayError};

/// Maps guest physical addresses into a host-backed arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTranslate {
    pub guest: GuestPageSize,
    pub host: HostPageSize,
}

impl PageTranslate {
    pub fn new(guest: GuestPageSize, host: HostPageSize) -> Result<Self, RelayError> {
        // Both sizes are already validated enums. Keep the constructor fallible
        // so future geometries can fail closed in one place.
        let _ = (guest.bytes(), host.bytes());
        Ok(Self { guest, host })
    }

    pub fn detect_host(guest: GuestPageSize) -> Result<Self, RelayError> {
        Self::new(guest, HostPageSize::detect())
    }

    /// Bytes of host arena needed to hold `guest_bytes` of guest RAM.
    /// Rounded up to a whole host page.
    pub fn host_arena_bytes(self, guest_bytes: u64) -> Result<u64, RelayError> {
        if guest_bytes == 0 {
            return Err(RelayError::Failed(
                "guest memory size must be non-zero".into(),
            ));
        }
        if !self.guest.is_aligned(guest_bytes) {
            return Err(RelayError::Failed(
                "guest memory size must be guest-page aligned".into(),
            ));
        }
        // Contiguous GPA→offset identity. Host arena is the same span, rounded
        // up so mmap / Vec capacity respects host pages.
        let host = self.host.0 as u64;
        let rounded = guest_bytes
            .checked_add(host - 1)
            .ok_or_else(|| RelayError::Failed("host arena size overflow".into()))?
            & !(host - 1);
        Ok(rounded)
    }

    /// Guest physical address → byte offset in the host arena.
    /// Identity mapping for the supported 4k/16k pairs (guest pages pack into
    /// larger host pages, or span multiple smaller host pages).
    pub fn guest_to_host_offset(self, gpa: u64) -> Result<u64, RelayError> {
        Ok(gpa)
    }

    /// How many host pages cover one guest page.
    pub fn host_pages_per_guest_page(self) -> u32 {
        let g = self.guest.0;
        let h = self.host.0;
        if g >= h {
            g / h
        } else {
            1
        }
    }

    /// How many guest pages fit in one host page (1 when guest >= host).
    pub fn guest_pages_per_host_page(self) -> u32 {
        let g = self.guest.0;
        let h = self.host.0;
        if h >= g {
            h / g
        } else {
            1
        }
    }

    pub fn pair_label(self) -> String {
        format!(
            "guest-{}k-on-host-{}k",
            self.guest.0 / 1024,
            self.host.0 / 1024
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_size_is_identity() {
        let t = PageTranslate::new(GuestPageSize::FOUR_KIB, HostPageSize::FOUR_KIB).unwrap();
        assert_eq!(t.host_arena_bytes(4096 * 4).unwrap(), 4096 * 4);
        assert_eq!(t.guest_to_host_offset(0x1000).unwrap(), 0x1000);
        assert_eq!(t.host_pages_per_guest_page(), 1);
        assert_eq!(t.guest_pages_per_host_page(), 1);
    }

    #[test]
    fn guest_4k_on_host_16k_packs() {
        let t = PageTranslate::new(GuestPageSize::FOUR_KIB, HostPageSize::SIXTEEN_KIB).unwrap();
        assert_eq!(t.guest_pages_per_host_page(), 4);
        assert_eq!(t.host_pages_per_guest_page(), 1);
        // 3 guest pages → round up to one host page of arena
        assert_eq!(t.host_arena_bytes(4096 * 3).unwrap(), 16384);
        assert_eq!(t.guest_to_host_offset(0x2000).unwrap(), 0x2000);
        assert_eq!(t.pair_label(), "guest-4k-on-host-16k");
    }

    #[test]
    fn guest_16k_on_host_4k_spans() {
        let t = PageTranslate::new(GuestPageSize::SIXTEEN_KIB, HostPageSize::FOUR_KIB).unwrap();
        assert_eq!(t.host_pages_per_guest_page(), 4);
        assert_eq!(t.guest_pages_per_host_page(), 1);
        assert_eq!(t.host_arena_bytes(16384).unwrap(), 16384);
        assert_eq!(t.guest_to_host_offset(0x4000).unwrap(), 0x4000);
    }

    #[test]
    fn rejects_unaligned_guest_size() {
        let t = PageTranslate::new(GuestPageSize::SIXTEEN_KIB, HostPageSize::SIXTEEN_KIB).unwrap();
        assert!(t.host_arena_bytes(4096).is_err());
    }

    #[test]
    fn all_four_pairs_construct() {
        for guest in [GuestPageSize::FOUR_KIB, GuestPageSize::SIXTEEN_KIB] {
            for host in [HostPageSize::FOUR_KIB, HostPageSize::SIXTEEN_KIB] {
                let t = PageTranslate::new(guest, host).unwrap();
                let arena = t.host_arena_bytes(guest.0 as u64 * 8).unwrap();
                assert!(arena >= guest.0 as u64 * 8);
                assert_eq!(arena % host.0 as u64, 0);
            }
        }
    }
}

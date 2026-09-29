#![no_main]

use libfuzzer_sys::fuzz_target;
use relay_core::{GuestPageSize, HostPageSize};
use relay_vm::PageTranslate;

fuzz_target!(|input: &[u8]| {
    if input.len() < 10 {
        return;
    }

    let mut raw = [0u8; 8];
    raw.copy_from_slice(&input[..8]);
    let guest_bytes = u64::from_le_bytes(raw);
    let guest = if input[8] & 1 == 0 {
        GuestPageSize::FOUR_KIB
    } else {
        GuestPageSize::SIXTEEN_KIB
    };
    let host = if input[9] & 1 == 0 {
        HostPageSize::FOUR_KIB
    } else {
        HostPageSize::SIXTEEN_KIB
    };
    let translation = PageTranslate::new(guest, host).unwrap();

    match translation.host_arena_bytes(guest_bytes) {
        Ok(arena) => {
            let host_bytes = host.0 as u64;
            assert_ne!(guest_bytes, 0);
            assert_eq!(guest_bytes % guest.0 as u64, 0);
            assert!(arena >= guest_bytes);
            assert_eq!(arena % host_bytes, 0);
            assert!(arena - guest_bytes < host_bytes);
        }
        Err(_) => {
            let invalid_input = guest_bytes == 0
                || guest_bytes % guest.0 as u64 != 0
                || guest_bytes.checked_add(host.0 as u64 - 1).is_none();
            assert!(invalid_input);
        }
    }
});

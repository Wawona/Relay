use vstd::prelude::*;

verus! {

/// Mathematical model for the only page-size pair that needs padding:
/// arbitrary 4 KiB guest pages packed into 16 KiB host pages.
proof fn guest_4k_on_host_16k_rounding(whole_host_pages: nat, remainder: nat)
    requires
        remainder < 4,
    ensures
        ({
            let guest_bytes = (4 * whole_host_pages + remainder) * 4096;
            let arena_bytes = if remainder == 0 {
                whole_host_pages * 16384
            } else {
                (whole_host_pages + 1) * 16384
            };
            &&& arena_bytes >= guest_bytes
            &&& arena_bytes % 16384 == 0
            &&& arena_bytes - guest_bytes < 16384
        }),
{
    if remainder == 0 {
        assert(4 * 4096 == 16384) by (nonlinear_arith);
    } else {
        assert(1 <= remainder && remainder <= 3);
        assert(4 * 4096 == 16384) by (nonlinear_arith);
    }
}

/// The other supported host/guest page pairs need no padding because the
/// guest byte count is already a multiple of the host page size.
proof fn naturally_aligned_page_pairs(pages: nat, guest_page: nat, host_page: nat)
    requires
        pages > 0,
        guest_page == 4096 || guest_page == 16384,
        host_page == 4096 || host_page == guest_page,
    ensures
        pages * guest_page >= pages * guest_page,
        (pages * guest_page) % host_page == 0,
        pages * guest_page - pages * guest_page < host_page,
{
    if guest_page == 4096 {
        assert(host_page == 4096);
    } else if host_page == 4096 {
        assert(guest_page == 16384);
        assert(16384 == 4 * 4096) by (compute);
        assert(guest_page == 4 * host_page);
    } else {
        assert(host_page == guest_page);
    }
}

fn main() {}

}

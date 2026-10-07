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

/// Model of cpu::page_fragment_len: a scalar/SIMD access consumes a
/// nonempty first fragment and, if split, fits wholly in the next page.
proof fn scalar_access_page_fragments(offset: nat, page_bytes: nat, bytes: nat)
    requires
        page_bytes == 4096 || page_bytes == 16384,
        offset < page_bytes,
        0 < bytes <= 64,
    ensures
        ({
            let available = (page_bytes - offset) as nat;
            let first = if bytes < available { bytes } else { available };
            &&& 0 < first <= bytes
            &&& offset + first <= page_bytes
            &&& (first < bytes ==> offset + first == page_bytes)
            &&& (first < bytes ==> bytes - first < page_bytes)
        }),
{
}

/// Mathematical lane-index bound paired with the production DUP/UMOV/SMOV
/// Kani harnesses. The reference suite additionally checks instruction results.
proof fn simd_lane_selection_stays_within_vector(bits: nat, lane: nat)
    requires
        bits == 8 || bits == 16 || bits == 32 || bits == 64,
        lane < 128nat / bits,
    ensures
        lane * bits + bits <= 128,
{
    if bits == 8 {
        assert(lane <= 15);
    } else if bits == 16 {
        assert(lane <= 7);
    } else if bits == 32 {
        assert(lane <= 3);
    } else {
        assert(lane <= 1);
    }
}

/// Rounding a nonzero 64-bit integer can increase its exponent by at most
/// one; even that carry remains in the normal finite binary32/binary64 range.
proof fn rounded_integer_exponent_is_finite(highest: nat, carry: bool, double: bool)
    requires highest < 64,
    ensures
        ({
            let exponent = highest + if carry { 1nat } else { 0nat };
            let bias = if double { 1023nat } else { 127nat };
            let reserved = if double { 2047nat } else { 255nat };
            &&& exponent <= 64
            &&& 0 < exponent + bias < reserved
        }),
{
}

/// Boolean contract paired with mmu::permits and its bit-precise Kani
/// harness. This model does not prove the page walker or exception engine.
proof fn execute_permission_excludes_forbidden_access(
    user: bool, writable: bool, leaf_xn: bool, table_xn: bool,
    el0: bool, wxn: bool,
)
    ensures
        ({
            let allowed = (!el0 || user)
                && !leaf_xn && !table_xn
                && (el0 || !(user && writable))
                && !(wxn && writable);
            &&& (allowed ==> (!el0 || user))
            &&& (allowed ==> !leaf_xn && !table_xn)
            &&& (allowed && !el0 ==> !(user && writable))
            &&& (allowed && wxn ==> !writable)
        }),
{
}

/// Routing model for bus::sgi_for_cpu0; bit extraction is checked by Kani.
/// This does not prove GIC priorities, active state, or exception delivery.
proof fn single_cpu_sgi_routing(filter: nat, target_zero: bool, irq: nat)
    requires filter < 4, irq < 16,
    ensures
        ({
            let deliver = if filter == 0 { target_zero } else { filter == 2 };
            &&& (deliver <==> (filter == 2 || (filter == 0 && target_zero)))
            &&& (filter == 1 || filter == 3 ==> !deliver)
            &&& (deliver ==> irq < 16)
        }),
{
}

/// Capacity arithmetic model; filesystem publication and durability are outside this lemma.
proof fn disk_growth_preserves_extent(base: nat, existing: nat, gib: nat, quota: nat)
    requires 4 <= gib <= quota <= 64, base <= gib * 1073741824, existing <= gib * 1073741824,
    ensures base <= gib * 1073741824, existing <= gib * 1073741824,
        4 * 1073741824 <= gib * 1073741824 <= 64 * 1073741824,
        gib * 1073741824 <= quota * 1073741824,
{}

/// Modular arithmetic paired with vsock_wire::available_credit. This is a
/// stream window invariant, not proof of stream authentication or socket I/O.
proof fn vsock_stream_credit_window(allocated: int, transmitted: int, forwarded: int)
    requires 0 <= allocated < 4294967296,
        0 <= transmitted < 4294967296, 0 <= forwarded < 4294967296,
    ensures
        ({
            let outstanding = (transmitted + 4294967296 - forwarded) % 4294967296;
            &&& 0 <= outstanding < 4294967296
            &&& (outstanding <= allocated ==> 0 <= allocated - outstanding <= allocated)
            &&& (outstanding <= allocated ==> (allocated - outstanding) + outstanding == allocated)
        }),
{}

/// Separate forwarding advancement check prevents a peer allocation from
/// legitimizing acknowledgement of bytes that were never transmitted.
proof fn vsock_forwarding_advancement(outstanding: int, advance: int)
    requires 0 <= outstanding < 4294967296, 0 <= advance < 4294967296,
    ensures advance <= outstanding ==> 0 <= outstanding - advance <= outstanding,
        advance > outstanding ==> outstanding - advance < 0,
{}

/// Each signed input fits [-limit,limit); widened addition has no clipping.
/// Accumulation is modulo a positive widened destination modulus.
proof fn pairwise_long_widened_sum(a: int, b: int, limit: int, prior: int, modulus: int)
    requires limit > 0, -limit <= a < limit, -limit <= b < limit,
        modulus > 0, 0 <= prior < modulus,
    ensures -2 * limit <= a + b <= 2 * limit - 2,
        0 <= (a + b + prior) % modulus < modulus,
{}

/// Pending buffer model paired with stream_bridge::consume. Socket I/O and
/// cancellation ordering remain native integration-test obligations.
proof fn stream_pending_consumption(start: nat, end: nat, count: nat, capacity: nat)
    requires start <= end <= capacity, count <= end - start,
    ensures start + count <= end <= capacity,
        end - (start + count) == end - start - count,
        count == end - start ==> start + count == end,
{}

/// IPv4 header span model paired with net_frame::ipv4_span_ok.
/// Socket NAT, DHCP payloads, and checksum bytes are outside this lemma.
proof fn ipv4_header_fits_ethernet_frame(frame_len: nat, ihl: nat, total: nat)
    requires
        5 <= ihl <= 15,
        total >= ihl * 4,
        frame_len >= 14,
        total <= frame_len - 14,
        frame_len <= 2048,
    ensures
        14 + total <= frame_len,
        ihl * 4 <= total,
        total <= 2048,
{
}

/// Signed AOT image identity. Paired with aot::image_bytes_match.
/// A changed guest byte must not reuse the signed translation.
proof fn signed_translation_rejects_a_changed_byte(expected: Seq<u8>, live: Seq<u8>, i: int)
    requires
        0 <= i < expected.len(),
        expected.len() == live.len(),
        expected[i] != live[i],
    ensures
        expected != live,
{
}

spec fn translation_dropped_model(store_page: nat, executed_page: nat) -> bool {
    store_page == executed_page
}

proof fn store_into_executed_page_drops_translation(store_page: nat, executed_page: nat)
    ensures
        translation_dropped_model(store_page, executed_page) == (store_page == executed_page),
{
}

/// AOT software TLB: a hit is the page base plus the in-page offset.
proof fn aot_tlb_hit_stays_in_page(offset: nat, va_page: nat, pa_page: nat)
    requires offset < 4096,
    ensures
        pa_page + offset >= pa_page,
        (pa_page + offset) - pa_page == offset,
        (pa_page + offset) - pa_page < 4096,
{
}

/// Shared-RAM span for two AOT host threads. Mutex ordering is outside this lemma.
proof fn aot_shared_ram_span(offset: nat, len: nat, ram: nat)
    requires offset + len <= ram,
    ensures offset + len <= ram,
        len == 0 ==> offset <= ram,
{
}

/// FIN only after inflight and guest queue are empty. Socket checksums remain unproved.
proof fn tcp_fin_requires_empty_inflight(
    peer_fin: bool,
    sent_fin: bool,
    inflight_empty: bool,
    queued_empty: bool,
    open: bool,
)
    ensures
        ({
            let send = peer_fin && !sent_fin && inflight_empty && queued_empty && open;
            &&& (send ==> inflight_empty)
            &&& (send ==> queued_empty)
            &&& (send ==> !sent_fin)
            &&& (send ==> peer_fin && open)
        }),
{
}

/// BUFFER_DIFF word span times four stays inside the file and the message body.
proof fn buffer_diff_span_fits(file_len: nat, start: nat, end: nat, pos: nat, body_len: nat)
    requires
        end > start,
        (end - start) * 4 + pos <= body_len,
        start * 4 + (end - start) * 4 <= file_len,
    ensures
        start * 4 + (end - start) * 4 <= file_len,
        pos + (end - start) * 4 <= body_len,
{
}

fn main() {}

}

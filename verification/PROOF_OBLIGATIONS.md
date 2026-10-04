# Relay VM proof obligations

This register states exactly what is proved and what is not. A passing gate is
evidence for these rows only. It is not a whole-VM correctness claim.

| Law | Production Rust | Bounded proof | Unbounded model | Runtime evidence |
|---|---|---|---|---|
| Host arena covers guest RAM, is host-page aligned, and pads by less than one host page | `page_translate::round_up_host_arena` | `host_arena_rounding_covers_aligned_guest_ram` | `guest_4k_on_host_16k_rounding`, `naturally_aligned_page_pairs` | All four 4/16 KiB pairs and rejection tests |
| Split low/high MMIO writes preserve the untouched half and assemble the requested 64-bit value | `virtio_mmio::set_low`, `virtio_mmio::set_high` | `queue_address_halves_reconstruct_every_u64` | Bit-vector identity is checked on production code by Kani; no separate integer model | MMIO queue configuration tests |
| Accepted block data spans are nonempty, sector aligned, bounded and overflow free | `virtio_block::checked_request_end` | `accepted_request_span_is_aligned_bounded_and_overflow_free`, `overflowing_request_span_is_rejected` | No separate model | Invalid-chain, scatter/gather and block metadata tests |
| A scalar/SIMD fragment stays within one translation granule | `cpu::page_fragment_len` | `page_fragment_stays_within_translation_granule` | `scalar_access_page_fragments` | Discontiguous 4/16 KiB accesses and fault tests |
| Valid lane moves, indexed FP operands and vector rounding operands respect widths, bounds and unsigned selection | `cpu::simd_lane_to_gpr`, `cpu::duplicate_simd_lane` | `simd_lane_move_width_and_bounds`, `duplicate_lane_fits_selected_vector_width` | `simd_lane_selection_stays_within_vector` | Native instruction-family corpus including 16 indexed FMUL S/D and 21 vector FRINT S/D forms, reserved encodings |
| FP narrowing preserves sign except default NaN, and only sets defined status bits | `float_convert::narrow_double` | `narrow_double_preserves_sign_and_limits_status` | No separate model | Native FP controls/corner cases, portable Miri boundaries |
| Finite unflushed f32 widening and narrowing round-trips exactly without exceptions | `float_convert::{widen_single,narrow_double}` | `widen_single_is_exact_for_finite_unflushed_inputs` | No separate model | Native precision conversions |
| Integer conversion cannot overflow the floating exponent range | `float_convert::integer_to_float` | `integer_conversion_cannot_overflow_floating_range` | `rounded_integer_exponent_is_finite` | Native signed/unsigned width/rounding corpus |
| FP compare returns a valid NZCV pattern and only IOC/IDC status | `float_arithmetic::compare` | `comparison_result_and_exception_domains` | No separate model | Native quiet/signaling comparisons |
| Executable mappings obey effective privilege, leaf/table XN and WXN | `mmu::permits` | `executable_mappings_obey_privilege_and_execute_never` | `execute_permission_excludes_forbidden_access` is a Boolean model, not a walker refinement proof | Instruction-fetch leaf/table/exception matrix on both page sizes |
| XN never restricts data access; table AP restrictions deny writes/EL0 access | `mmu::permits` | `data_permissions_ignore_execute_never_and_respect_tables` | No separate model | Inherited data restrictions on both page sizes |
| Software interrupts target CPU 0 only for its explicit bit or self filter; IDs stay below 16 | `bus::sgi_for_cpu0` | `software_interrupt_targets_only_the_single_cpu` | `single_cpu_sgi_routing`, routing model only | All 16 IDs, all filters, representative target masks, acknowledge/EOI; Miri |

## Preconditions

- SGI routing assumes one CPU interface and no Security Extensions. GIC masks,
  priorities, active state and scheduling liveness remain outside this property.
- Guest and host page sizes are validated 4 KiB or 16 KiB values.
- MMIO split writes use two 32-bit register values for one 64-bit address.
- Arena Kani harness uses 1..=65535 guest pages; the paired Verus model is
  unbounded. That does not automatically extend the production-code proof.
- Scalar/SIMD fragments have widths 1..=64; DUP has unwind bound 17 and
  validated element/vector widths. Other harness assumptions live beside code.
- Permission helpers cover baseline EL1&0 direct permissions, with HPDS and
  permission indirection absent from the advertised CPU. They do not prove
  access flags, all descriptor formats, canonical-address handling or PAN.
- FP properties above are intentionally narrower than full IEEE/ARM FP
  semantics. The software FP dependency is not proved by these harnesses.

## Not proved

- Whole Linux boot, guest kernel correctness, or device-driver correctness.
- Liveness, fairness, scheduling, interrupt delivery timing, or absence of
  deadlock across the whole VM.
- Virtualization.framework, KVM, Hypervisor.framework, Metal, Swift, Kotlin,
  Objective-C, JNI, or C implementation correctness.
- Compiler, linker, operating system, hardware, or supply-chain correctness.
- Equivalence between the Verus mathematical model and every future production
  refactor. Review must maintain the named pairing.

Fuzzing, Miri, sanitizers, Clippy, OSV, cargo-deny, CodeQL, and tests add
independent evidence. They do not expand the mathematical proof boundary.

## Immediate-shift lane kernel

`cpu::simd_immediate_shift` is checked by
`immediate_shift_lane_bounds_and_full_width_edges`: arbitrary source/destination
bits, widths 8/16/32/64, the six non-saturating operation classes, left shifts
0..width-1 and right shifts 1..width. Results stay within the lane; shift-zero
left, full-width insertion and full-width rounded right edges have exact
identities. Kani also checks Rust shift/overflow safety on this valid domain.
There is no separate Verus model for this kernel. Decoder selection and general
ARM equivalence use the native corpus, not this narrower proof.

## Integer reduction kernel

`cpu::simd_reduce` shares ADDV, S/UADDLV, S/UMAXV and S/UMINV execution.
`integer_reductions_match_scalar_specification` checks valid arrangements for
arbitrary source bits, signedness and reduction kind, with unwind 17. It proves
Rust arithmetic/shift safety, scalar width, sums modulo output width against an
independent i128 interpretation, and min/max by input membership and ordering.
This covers the finite integer lane kernel, not decoder/ISA equivalence; 35 fixed
native instruction forms provide independent result/flag comparisons. Reserved arrangements and
in-place UMAXV are portable regressions also run under Miri.

## Table lookup kernel

`cpu::simd_table_lookup` concatenates one to four vector tables for TBL/TBX.
`table_lookup_matches_selected_byte` proves, for arbitrary tables, indices,
prior destination and a selected active byte, exact table selection or the
specified zero/preserved out-of-range result. It also proves Q0 upper clearing
and Rust bounds/shift safety (unwind 17). This is a pure lane-kernel property,
not a decoder or whole-CPU refinement proof. Sixteen fixed native forms cover
both widths, all table counts, TBL/TBX, boundaries and wrapped in-place tables.
A portable destination/index overlap regression runs under Miri.

## Saturating arithmetic kernel

`cpu::saturating_lane` implements signed/unsigned add/subtract at 8/16/32/64 bits.
`saturating_arithmetic_matches_integer_bounds` proves exact mathematical clamping,
width and saturation indication against an independent bit interpretation, for
arbitrary inputs. The dispatcher ORs lane indications into sticky FPSR.QC; 44
native scalar/vector forms compare complete results and FPSR with QC clear/set.
In-place subtraction, sticky status and reserved widths are portable Miri cases.
Scalar ADDP Dd,Vn.2D uses modulo-u64 addition, with native and alias regressions;
this separate instruction has no new dedicated Kani theorem.

## Interleaved structure layout

LD/ST2..4 reuse the existing 64-byte preflight/transfer path. The pure byte
mapping `structure_byte_offset` is checked by
`structure_byte_layout_is_bounded_and_invertible` for all supported element,
vector and register counts. Bounds plus the demonstrated inverse establish a
permutation of the transferred bytes. Another 126 fixed native forms exercise
load/store, all valid arrangements and no/immediate/register writeback. Existing
every-page-split, register-wrap and no-partial-store fault tests cover both
consecutive LD/ST1 and interleaved LD/ST4 on both page sizes, including Miri.

## Console transfer boundary

Per-queue notifications retain deferred RX while TX progresses. The console
preflights every descriptor direction and RAM span before payload effects, uses
a fixed 4 KiB scratch buffer, and reports only device-written used lengths.
Portable tests/Miri cover interleaved kicks, delayed input, malformed second
descriptors, oversized lengths, chunk boundaries and zero-sized output quotas.
This is executable regression evidence, not a queue/lifecycle refinement proof.
High-lane FMOV has native/portable bit-transfer tests, with no dedicated theorem.

## Integer min/max selection

Elementwise SMAX/UMAX/SMIN/UMIN reuse the existing pairwise lane path.
`integer_minmax_selects_ordered_input` proves the shared selector returns one
masked operand with the required ordering, for arbitrary inputs and all legal
8/16/32-bit widths. Native checks cover 24 elementwise arrangements; the earlier
pairwise corpus remains active. Alias/reserved-width tests run under Miri.
This is a lane theorem plus finite decoder evidence, not full ISA equivalence.

OCI process conversion now preserves explicit numeric User uid:gid and rejects
account resolution it cannot yet perform, as well as empty executable commands.
Pure conversion tests run under Miri. No identity-resolution or extraction
confinement theorem is claimed.

### Vector leading-bit counts

`leading_count_matches_prefix` proves CLZ/CLS counts for arbitrary 8/16/32-bit
elements: every counted bit matches the zero/sign prefix and the next bit, when
present, differs. Twelve fixed native instruction forms cover both vector widths;
portable tests cover destination aliasing, boundary values and reserved D forms.
This lemma proves the lane helper, not whole-guest correctness.

## Pairwise widened arithmetic and vsock credit

`cpu::pairwise_long_lane` accepts source widths8/16/32. Signed/unsigned pairs
add exactly into twice the width; ADALP accumulates modulo that widened width.
`pairwise_long_add_matches_widened_modular_sum` proves the actual helper against
an independent i128 reference. Verus `pairwise_long_widened_sum` models signed
sum bounds and modular accumulation; it is not a decoder/ISA refinement proof.
Native fixed forms cover both vector sizes, signs, accumulation and aliases;
portable tests reject reserved size3, check the measured aliased UADDLP, upper
clearing and flags. Full CPU/boot correctness remains outside these proofs.

`vsock_wire::available_credit` and `vsock::PeerCredit::update` have production
Kani harnesses `stream_credit_never_exceeds_peer_window` and
`forwarding_cannot_acknowledge_unsent_bytes`. The Verus models are
`vsock_stream_credit_window` and `vsock_forwarding_advancement`. Reject advances
beyond prior outstanding bytes independently of the peer allocation; accepted
windows conserve allocated credit. Socket IO, queue liveness, peer identity,
authentication, frame import and full concurrency are not proved by these laws.

## Connected socket bridge

`stream_bridge::consume` validates pending ranges before mutation. Production
Kani `consumption_preserves_bounded_pending_bytes` checks arbitrary usize inputs,
invalid ranges, consumption bounds and exact remaining bytes. The separate
Verus `stream_pending_consumption` models the valid-range arithmetic. Neither
proves socket behavior or thread scheduling. Real Unix socket tests cover
bidirectional backpressure, EOF drain with reverse traffic, cancellation/drop
and fatal peer errors. Each direction buffers at most64KiB and each advance
performs bounded nonblocking operations. This core owns connected endpoints;
native waypipe launch, listener connection, guest authentication and imported
frame acceptance remain unresolved and must not be inferred from byte copying.

## Native host channel worker

The StaticCpu host worker consumes real registered vsock connections serially.
A synchronous native entry borrows its live Unix descriptor and must duplicate
anything it owns. CPU shutdown drops the device peer before native worker join;
join has a bounded observation timeout, and a delayed entry stays registered
with its handle so Stop can retry. Tests cover actual stream callbacks,
reconnect, EOF during a partial read, pre-connection stop and delayed ownership.
These are native concurrency/ownership tests, not formal proofs of foreign
waypipe, OS scheduling, authentication or imported frames. Existing Kani/Verus
invariants remain required for the source that runs these checks. Native patch
passes invocation-local arguments to the original parser and routes a duplicated
connected channel to the original handle_client_conn with the real host display.

Indexed FMUL reuses `simd_lane_to_gpr` and the existing software-FP multiply.
Kani checks exact unsigned lane selection; Verus bounds the existing lane model.
The decoder and FP arithmetic are compared against fixed native instructions,
not formally proved as whole-ISA equivalence. Guest target/frame remain separate.


## Vector integral rounding integration

Vector FRINT N/P/M/Z/A/X/I uses the existing `simd_lane_to_gpr` unsigned
selection law and `float_arithmetic::round_integral` software-FP semantics.
For 2S/4S/2D, each selected lane fits 128 bits; Q0/D rejects before mutation,
source bits are captured before an aliased write, inactive lanes are not read,
Q0 clears upper bits, and per-lane exceptions accumulate into sticky FPSR.
Existing Kani `simd_lane_move_width_and_bounds` and Verus
`simd_lane_selection_stays_within_vector` cover extraction bounds/selection.
They do not prove the decoder, FP rounding or whole-instruction refinement.
21 fixed native forms compare 190848 cases across RMode/FZ/DN, mixed lanes,
zeros, infinities, NaNs, ties, subnormals and deterministic random bits.
Portable measured FRINTA/aliased/inactive-sNaN and reserved-encoding regressions
run under strict Miri. Reference: Arm Instruction Set Reference Guide,
https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab .

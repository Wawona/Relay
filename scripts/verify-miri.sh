#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

expected="$($repo_root/scripts/formal-source-digest.sh)"
actual="$(test -f .wawona-verification.stamp && sed -n '1p' .wawona-verification.stamp || true)"
if [[ "$actual" != "$expected" ]]; then
  echo "Miri blocked: Kani and Verus proof stamp is missing or stale." >&2
  exit 1
fi

export MIRIFLAGS="${MIRIFLAGS:--Zmiri-strict-provenance}"
# Keep the supported pure-Rust boundary suites in the gate. Native instruction
# reference tests have their own ARM64 job; Miri cannot execute their asm.
for suite in \
  page_translate \
  mmu::tests \
  bus::tests \
  hardware_el1_checkpoints_ \
  data_access_flag_ \
  cpu::tests::structure_ \
  cpu::tests::fmov_ \
  cpu::native_reference::immediate_shift_boundaries_ \
  cpu::native_reference::structure_lane_ \
  cpu::native_reference::scalar_dup_in_place_ \
  instruction_fetch_ \
  table_permissions_ \
  float_convert::tests \
  float_arithmetic::tests \
  cpu::tests::scalar_fused_ \
  cpu::native_reference::indexed_float::indexed_fmul_measured_ \
  cpu::native_reference::indexed_float::indexed_fmul_reserved_ \
  cpu::native_reference::vector_round::vector_round_ \
  cpu::native_reference::vector_precision::vector_precision_ \
  cpu::native_reference::pairwise_long_guest_ \
  virtio_block::metadata_tests \
  storage::tests::capacity_ \
  stream_bridge::tests::pending_range_ \
  shutdown::tests \
  cpu::native_reference::conditional_float_compare_skips_ \
  virtio_console::tests \
  virtio_vsock::tests::entire_chain_ \
  vsock::tests::credit_wrap_ \
  vsock::tests::malformed_headers_ \
  vsock_wire::tests \
  cpu::native_reference::integer_reductions_alias_ \
  cpu::native_reference::integer_minmax_aliases_ \
  cpu::native_reference::leading_count_boundaries_ \
  cpu::native_reference::logical_immediate_stack_ \
  cpu::native_reference::table_lookup_boundaries_ \
  cpu::native_reference::saturating_arithmetic_aliases_ \
  cpu::native_reference::reserved_ \
  aot::tests::software_tlb_ \
  aot::tests::two_host_threads_ \
  shm_import::tests::buffer_diff_
do
  cargo "+${MIRI_TOOLCHAIN:-nightly}" miri test -p relay-vm --locked "$suite"
done

# Pure OCI process conversion: no archive/foreign decompressor execution.
cargo "+${MIRI_TOOLCHAIN:-nightly}" miri test -p relay-oci --locked materialize::tests::process_

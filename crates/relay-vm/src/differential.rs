//! Deterministic checkpoints for differential StaticCpu boot diagnosis.
//!
//! A native AArch64 or QEMU reference recorder outside the product emits the
//! same JSON-lines schema. Relay records its interpreter at identical interpreter
//! step counts. Cumulative hashes make the first divergent checkpoint
//! monotonic, so a comparator can locate it with binary search.

use crate::linux_boot;
use relay_core::{GuestManifest, RelayError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
};

pub const TRACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyPageHash {
    pub physical_address: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceCheckpoint {
    pub schema_version: u32,
    pub sequence: u64,
    pub instructions: u64,
    pub pc: String,
    pub physical_pc: String,
    pub x: Vec<String>,
    pub vectors: Vec<String>,
    pub sp: String,
    pub sp_el1: String,
    pub nzcv: u8,
    pub system_registers: Vec<(String, String)>,
    pub dirty_pages: Vec<DirtyPageHash>,
    pub state_sha256: String,
    pub chain_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceConfig {
    pub checkpoint_interval: u64,
    pub max_instructions: u64,
}

impl TraceConfig {
    pub fn validate(self) -> Result<Self, RelayError> {
        if self.checkpoint_interval == 0 || self.max_instructions == 0 {
            return Err(RelayError::Failed(
                "differential trace counts must be non-zero".into(),
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceDivergence {
    pub checkpoint_index: usize,
    pub reference_instructions: Option<u64>,
    pub actual_instructions: Option<u64>,
    pub reason: String,
}

pub fn record_static_trace(
    manifest: &GuestManifest,
    config: TraceConfig,
    output: &Path,
) -> Result<u64, RelayError> {
    let config = config.validate()?;
    let (mut cpu, _) = linux_boot::create_cpu(manifest)?;
    cpu.begin_differential_trace();
    let file = File::create(output)
        .map_err(|error| RelayError::Failed(format!("cannot create trace: {error}")))?;
    let mut writer = BufWriter::new(file);
    let mut previous_chain = [0u8; 32];
    let mut instructions = 0u64;
    let mut sequence = 0u64;

    while instructions < config.max_instructions {
        let count = config
            .checkpoint_interval
            .min(config.max_instructions - instructions);
        cpu.run_slice(count)?;
        instructions += count;
        let checkpoint = cpu.differential_checkpoint(sequence, instructions, previous_chain)?;
        previous_chain = decode_hash(&checkpoint.chain_sha256)?;
        serde_json::to_writer(&mut writer, &checkpoint)
            .map_err(|error| RelayError::Failed(format!("cannot encode trace: {error}")))?;
        writer
            .write_all(b"\n")
            .map_err(|error| RelayError::Failed(format!("cannot write trace: {error}")))?;
        sequence += 1;
    }
    writer
        .flush()
        .map_err(|error| RelayError::Failed(format!("cannot flush trace: {error}")))?;
    Ok(sequence)
}

pub fn read_trace(path: &Path) -> Result<Vec<TraceCheckpoint>, RelayError> {
    let file = File::open(path)
        .map_err(|error| RelayError::Failed(format!("cannot open trace: {error}")))?;
    read_trace_from(BufReader::new(file))
}

fn read_trace_from(reader: impl BufRead) -> Result<Vec<TraceCheckpoint>, RelayError> {
    let mut checkpoints = Vec::new();
    let mut previous_chain = [0; 32];
    let mut previous_instructions = 0;
    for (index, line) in reader.lines().enumerate() {
        let line =
            line.map_err(|error| RelayError::Failed(format!("cannot read trace: {error}")))?;
        let checkpoint: TraceCheckpoint = serde_json::from_str(&line).map_err(|error| {
            RelayError::Failed(format!("invalid trace line {}: {error}", index + 1))
        })?;
        validate_checkpoint(
            &checkpoint,
            checkpoints.len() as u64,
            previous_instructions,
            previous_chain,
        )
        .map_err(|error| RelayError::Failed(format!("trace line {}: {error}", index + 1)))?;
        previous_chain = decode_hash(&checkpoint.chain_sha256)?;
        previous_instructions = checkpoint.instructions;
        checkpoints.push(checkpoint);
    }
    if checkpoints.is_empty() {
        return Err(RelayError::Failed("trace contains no checkpoints".into()));
    }
    Ok(checkpoints)
}

fn validate_checkpoint(
    checkpoint: &TraceCheckpoint,
    sequence: u64,
    previous_instructions: u64,
    previous_chain: [u8; 32],
) -> Result<(), RelayError> {
    let invalid = |reason: &str| RelayError::Failed(reason.into());
    if checkpoint.schema_version != TRACE_SCHEMA_VERSION {
        return Err(invalid("unsupported trace schema"));
    }
    if checkpoint.sequence != sequence || checkpoint.instructions <= previous_instructions {
        return Err(invalid(
            "checkpoint sequence or instruction count is out of order",
        ));
    }
    if checkpoint.x.len() != 31 || checkpoint.vectors.len() != 32 || checkpoint.nzcv > 15 {
        return Err(invalid("invalid architectural register shape"));
    }
    let fixed_hex = |value: &str, digits: usize| {
        value.len() == digits + 2
            && value.starts_with("0x")
            && value.as_bytes()[2..]
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    };
    for value in [
        &checkpoint.pc,
        &checkpoint.physical_pc,
        &checkpoint.sp,
        &checkpoint.sp_el1,
    ]
    .into_iter()
    .chain(checkpoint.x.iter())
    .chain(checkpoint.system_registers.iter().map(|(_, value)| value))
    {
        if !fixed_hex(value, 16) {
            return Err(invalid("invalid 64-bit register encoding"));
        }
    }
    if checkpoint.vectors.iter().any(|value| !fixed_hex(value, 32)) {
        return Err(invalid("invalid vector register encoding"));
    }
    let mut names = std::collections::BTreeSet::new();
    for (name, _) in &checkpoint.system_registers {
        if name.is_empty() || !names.insert(name) {
            return Err(invalid("empty or duplicate system register name"));
        }
    }
    let mut previous_page = None;
    for page in &checkpoint.dirty_pages {
        if !fixed_hex(&page.physical_address, 16) {
            return Err(invalid("invalid dirty-page address"));
        }
        let address = u64::from_str_radix(&page.physical_address[2..], 16)
            .map_err(|_| invalid("invalid dirty-page address"))?;
        if address % 4096 != 0 || previous_page.is_some_and(|previous| address <= previous) {
            return Err(invalid("dirty pages must be aligned, unique, and ordered"));
        }
        decode_hash(&page.sha256)?;
        previous_page = Some(address);
    }
    decode_hash(&checkpoint.state_sha256)?;
    decode_hash(&checkpoint.chain_sha256)?;
    let expected = finish_checkpoint(checkpoint.clone(), previous_chain)?;
    if expected.state_sha256 != checkpoint.state_sha256 {
        return Err(invalid("trace state hash mismatch"));
    }
    if expected.chain_sha256 != checkpoint.chain_sha256 {
        return Err(invalid("trace chain hash mismatch"));
    }
    Ok(())
}

/// Compare checkpoints already validated by `read_trace`.
pub fn first_divergence(
    reference: &[TraceCheckpoint],
    actual: &[TraceCheckpoint],
) -> Option<TraceDivergence> {
    let common = reference.len().min(actual.len());
    if common == 0 {
        return (reference.len() != actual.len()).then(|| TraceDivergence {
            checkpoint_index: 0,
            reference_instructions: reference.first().map(|item| item.instructions),
            actual_instructions: actual.first().map(|item| item.instructions),
            reason: "one trace is empty".into(),
        });
    }
    if reference[common - 1].chain_sha256 == actual[common - 1].chain_sha256 {
        return (reference.len() != actual.len()).then(|| TraceDivergence {
            checkpoint_index: common,
            reference_instructions: reference.get(common).map(|item| item.instructions),
            actual_instructions: actual.get(common).map(|item| item.instructions),
            reason: "trace lengths differ after matching prefix".into(),
        });
    }

    let mut low = 0usize;
    let mut high = common - 1;
    while low < high {
        let middle = low + (high - low) / 2;
        if reference[middle].chain_sha256 == actual[middle].chain_sha256 {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let reference_checkpoint = &reference[low];
    let actual_checkpoint = &actual[low];
    Some(TraceDivergence {
        checkpoint_index: low,
        reference_instructions: Some(reference_checkpoint.instructions),
        actual_instructions: Some(actual_checkpoint.instructions),
        reason: describe_difference(reference_checkpoint, actual_checkpoint),
    })
}

fn describe_difference(reference: &TraceCheckpoint, actual: &TraceCheckpoint) -> String {
    if reference.instructions != actual.instructions {
        return "checkpoint instruction counts differ".into();
    }
    if reference.pc != actual.pc {
        return format!(
            "pc differs: reference={} actual={}",
            reference.pc, actual.pc
        );
    }
    if reference.x != actual.x {
        let register = reference
            .x
            .iter()
            .zip(&actual.x)
            .position(|(left, right)| left != right)
            .unwrap_or(reference.x.len().min(actual.x.len()));
        return format!("x{register} differs");
    }
    if reference.dirty_pages != actual.dirty_pages {
        return "dirty-page hashes differ".into();
    }
    "architectural state differs".into()
}

pub(crate) fn finish_checkpoint(
    mut checkpoint: TraceCheckpoint,
    previous_chain: [u8; 32],
) -> Result<TraceCheckpoint, RelayError> {
    checkpoint.state_sha256.clear();
    checkpoint.chain_sha256.clear();
    let encoded = serde_json::to_vec(&checkpoint)
        .map_err(|error| RelayError::Failed(format!("cannot hash trace state: {error}")))?;
    let state: [u8; 32] = Sha256::digest(encoded).into();
    let mut chain = Sha256::new();
    chain.update(previous_chain);
    chain.update(state);
    checkpoint.state_sha256 = hex(&state);
    checkpoint.chain_sha256 = hex(&chain.finalize());
    Ok(checkpoint)
}

pub(crate) fn hex_u64(value: u64) -> String {
    format!("{value:#018x}")
}

pub(crate) fn hex_u128(value: u128) -> String {
    format!("{value:#034x}")
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hash(value: &str) -> Result<[u8; 32], RelayError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RelayError::Failed("trace hash is not SHA-256".into()));
    }
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| RelayError::Failed("trace hash is not hexadecimal".into()))?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint(sequence: u64, pc: u64, previous: [u8; 32]) -> TraceCheckpoint {
        finish_checkpoint(
            TraceCheckpoint {
                schema_version: TRACE_SCHEMA_VERSION,
                sequence,
                instructions: (sequence + 1) * 100,
                pc: hex_u64(pc),
                physical_pc: hex_u64(pc),
                x: vec![hex_u64(0); 31],
                vectors: vec![hex_u128(0); 32],
                sp: hex_u64(0),
                sp_el1: hex_u64(0),
                nzcv: 0,
                system_registers: Vec::new(),
                dirty_pages: Vec::new(),
                state_sha256: String::new(),
                chain_sha256: String::new(),
            },
            previous,
        )
        .unwrap()
    }

    fn trace(pcs: &[u64]) -> Vec<TraceCheckpoint> {
        let mut chain = [0u8; 32];
        pcs.iter()
            .enumerate()
            .map(|(index, pc)| {
                let item = checkpoint(index as u64, *pc, chain);
                chain = decode_hash(&item.chain_sha256).unwrap();
                item
            })
            .collect()
    }

    #[test]
    fn cumulative_hash_binary_searches_first_divergence() {
        let reference = trace(&[4, 8, 12, 16, 20]);
        let actual = trace(&[4, 8, 99, 16, 20]);
        let divergence = first_divergence(&reference, &actual).unwrap();
        assert_eq!(divergence.checkpoint_index, 2);
        assert!(divergence.reason.starts_with("pc differs"));
    }

    #[test]
    fn matching_traces_have_no_divergence() {
        let reference = trace(&[4, 8, 12]);
        assert_eq!(first_divergence(&reference, &reference), None);
    }

    fn encoded_trace(checkpoints: &[TraceCheckpoint]) -> Vec<u8> {
        let mut output = Vec::new();
        for checkpoint in checkpoints {
            serde_json::to_writer(&mut output, checkpoint).unwrap();
            output.push(b'\n');
        }
        output
    }

    #[test]
    fn trace_reader_validates_round_trip() {
        let checkpoints = trace(&[4, 8, 12]);
        assert_eq!(
            read_trace_from(encoded_trace(&checkpoints).as_slice()).unwrap(),
            checkpoints
        );
    }

    #[test]
    fn changed_state_cannot_hide_behind_original_hashes() {
        let mut checkpoints = trace(&[4, 8, 12]);
        checkpoints[0].x[5] = hex_u64(42);
        let error = read_trace_from(encoded_trace(&checkpoints).as_slice()).unwrap_err();
        assert!(error.to_string().contains("state hash mismatch"));
    }

    #[test]
    fn locally_rehashed_change_cannot_rejoin_original_chain() {
        let mut checkpoints = trace(&[4, 8, 12]);
        checkpoints[0].x[5] = hex_u64(42);
        checkpoints[0] = finish_checkpoint(checkpoints[0].clone(), [0; 32]).unwrap();
        let error = read_trace_from(encoded_trace(&checkpoints).as_slice()).unwrap_err();
        assert!(error.to_string().contains("chain hash mismatch"));
    }

    #[test]
    fn trace_reader_rejects_missing_reordered_and_duplicate_checkpoints() {
        let checkpoints = trace(&[4, 8, 12]);
        for invalid in [
            vec![checkpoints[1].clone()],
            vec![checkpoints[0].clone(), checkpoints[2].clone()],
            vec![checkpoints[0].clone(), checkpoints[0].clone()],
        ] {
            assert!(read_trace_from(encoded_trace(&invalid).as_slice()).is_err());
        }
    }

    #[test]
    fn trace_reader_rejects_empty_and_blank_evidence() {
        for bytes in [b"".as_slice(), b"\n", b"   \n"] {
            assert!(read_trace_from(bytes).is_err());
        }
    }

    #[test]
    fn trace_reader_rejects_invalid_shapes_even_when_rehashed() {
        let original = checkpoint(0, 4, [0; 32]);
        let mut invalid = original.clone();
        invalid.x.pop();
        let mut bad_vector = original.clone();
        bad_vector.vectors[0] = "0x1".into();
        let mut bad_flags = original.clone();
        bad_flags.nzcv = 16;
        let mut duplicate_pages = original.clone();
        duplicate_pages.dirty_pages = vec![
            DirtyPageHash {
                physical_address: hex_u64(4096),
                sha256: hex(&[0; 32]),
            };
            2
        ];
        for item in [invalid, bad_vector, bad_flags, duplicate_pages] {
            let item = finish_checkpoint(item, [0; 32]).unwrap();
            assert!(read_trace_from(encoded_trace(&[item]).as_slice()).is_err());
        }
    }

    #[test]
    fn malformed_unicode_hash_returns_error_without_panicking() {
        // 64 bytes, with a multi-byte character crossing an old slice boundary.
        let hash = format!("aé{}", "0".repeat(61));
        assert_eq!(hash.len(), 64);
        assert!(decode_hash(&hash).is_err());
    }
}

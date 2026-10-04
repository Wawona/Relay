//! Bounded development-only kernel observations. Never reads MMIO or edits RAM.
use super::StaticCpu;
use relay_core::GuestManifest;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Deserialize)]
pub struct Config {
    pub instructions: u64,
    pub pcs: Vec<u64>,
    pub addresses: Vec<u64>,
    /// Register indices whose RAM pointers are inspected; defaults to X0/X1/X2.
    #[serde(default)]
    pub registers: Option<Vec<usize>>,
    /// Extra byte offsets from selected registers (e.g. task fields).
    #[serde(default)]
    pub register_offsets: Vec<u64>,
    /// Also sample every N hits of each selected PC; zero keeps logarithmic sampling.
    #[serde(default)]
    pub sample_every_hits: u64,
}

#[derive(Serialize)]
struct Sample {
    instruction: u64,
    pc: u64,
    registers: [u64; 31],
    sp: u64,
    sp_el0: u64,
    elr_el1: u64,
    memory: Vec<(u64, Vec<Option<u64>>)>,
}

fn words(cpu: &StaticCpu, base: u64) -> Vec<Option<u64>> {
    (0..32)
        .map(|i| {
            let address = base.checked_add(i * 8)?;
            // Translate each byte independently: a sample may cross a guest page.
            // GuestMemory alone handles the read, so device registers cannot be consumed.
            let mut bytes = [0; 8];
            for (offset, byte) in bytes.iter_mut().enumerate() {
                let physical = cpu.physical(address.checked_add(offset as u64)?).ok()?;
                cpu.memory.read(physical, std::slice::from_mut(byte)).ok()?;
            }
            Some(u64::from_le_bytes(bytes))
        })
        .collect()
}

pub fn record(
    manifest: &GuestManifest,
    config: Config,
    output: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    if config.instructions == 0
        || config.pcs.len() > 32
        || config.addresses.len() > 16
        || config.register_offsets.len() > 8
        || config
            .registers
            .as_ref()
            .is_some_and(|r| r.len() > 8 || r.iter().any(|i| *i >= 31))
        || (config.sample_every_hits != 0 && config.sample_every_hits < 16)
    {
        return Err("probe requires positive budget, at most 32 PCs/16 addresses/8 offsets, and hit stride zero or >=16".into());
    }
    let (mut cpu, _) = crate::linux_boot::create_cpu(manifest)?;
    let mut hits = vec![0u64; config.pcs.len()];
    for instruction in 0..config.instructions {
        let entry = config.pcs.iter().position(|pc| *pc == cpu.pc);
        let selected = entry.is_some_and(|index| {
            hits[index] += 1;
            hits[index] <= 16
                || hits[index].is_power_of_two()
                || (config.sample_every_hits != 0 && hits[index] % config.sample_every_hits == 0)
        });
        if selected || instruction % 100_000_000 == 0 {
            let mut addresses = config.addresses.clone();
            if selected {
                for index in config.registers.as_deref().unwrap_or(&[0, 1, 2]) {
                    let base = cpu.x[*index];
                    addresses.push(base);
                    addresses.extend(
                        config
                            .register_offsets
                            .iter()
                            .filter_map(|offset| base.checked_add(*offset)),
                    );
                }
            }
            let mut memory = Vec::new();
            for address in addresses {
                let data = words(&cpu, address);
                for pointer in data.iter().take(4).flatten().copied() {
                    if pointer >> 48 == 0xffff && pointer != address {
                        memory.push((pointer, words(&cpu, pointer)));
                    }
                }
                memory.push((address, data));
            }
            let sample = Sample {
                instruction,
                pc: cpu.pc,
                registers: cpu.x,
                sp: cpu.sp,
                sp_el0: cpu.sysregs.sp_el0,
                elr_el1: cpu.sysregs.elr_el1,
                memory,
            };
            serde_json::to_writer(&mut *output, &sample)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
        cpu.run_slice(1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_reads_ram_without_device_or_fault_side_effects() {
        let memory = crate::guest::GuestMemory::allocate_on_host(
            relay_core::GuestPageSize::FOUR_KIB,
            relay_core::HostPageSize::SIXTEEN_KIB,
            4096,
        )
        .unwrap();
        let mut cpu = StaticCpu::new(memory, 0).unwrap();
        cpu.memory.write(4088, &42u64.to_le_bytes()).unwrap();
        let sample = words(&cpu, 4088);
        assert_eq!(sample[0], Some(42));
        assert!(sample[1..].iter().all(Option::is_none));
        assert!(words(&cpu, crate::bus::PL011_BASE)
            .iter()
            .all(Option::is_none));
        assert!(words(&cpu, u64::MAX).iter().all(Option::is_none));
        assert!(cpu.pending_data_abort.get().is_none());
        assert_eq!(cpu.pc, 0);
    }
}

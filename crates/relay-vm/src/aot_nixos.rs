//! Offline NixOS boot profile and hot-kernel AOT (feature `aot-translate`).
//!
//! Profiles a StaticCpu boot, maps hot guest pages back onto the bundled
//! ARM64 Image when possible, and builds a sparse [`AotImage`]. The translator
//! never ships in the store staticlib. No speed claim.

use crate::aot::{run_image, translate_regions, AotStats};
use crate::cpu::StaticCpu;
use crate::linux_boot::{self, read_image_header};
use crate::guest::GUEST_RAM_BASE;
use relay_core::{GuestManifest, RelayError};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::time::Instant;

const PAGE: u64 = 4096;
const ARM64_IMAGE_RELOCATABLE: u64 = 1 << 3;
const KERNEL_BASE_ALIGNMENT: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct NixosBootProfile {
    pub instructions: u64,
    pub unique_pages: usize,
    pub unique_pcs: usize,
    /// Guest VA page base -> hit count.
    pub page_hits: BTreeMap<u64, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NixosAotMeasure {
    pub profile_instructions: u64,
    pub hot_pages: usize,
    pub kernel_pages: usize,
    pub non_kernel_pages: usize,
    pub skipped_non_kernel_pages: usize,
    pub translator_ms: u128,
    pub signed_text_bytes: usize,
    pub blocks: usize,
    pub measure_budget: u64,
    pub aot_insns: u64,
    pub fallback_insns: u64,
    pub aot_coverage_pct: f64,
    pub nixos_kernel_translated: bool,
    /// When true, measure continued on the profile CPU (upper bound).
    pub same_boot_measure: bool,
}

fn kernel_load_address(text_offset: u64, flags: u64) -> Result<u64, RelayError> {
    if flags & ARM64_IMAGE_RELOCATABLE == 0 {
        return Err(RelayError::Failed(
            "Relay StaticCpu requires a relocatable ARM64 Image".into(),
        ));
    }
    GUEST_RAM_BASE
        .checked_add(KERNEL_BASE_ALIGNMENT)
        .and_then(|base| base.checked_add(text_offset))
        .ok_or_else(|| RelayError::Failed("guest kernel load address overflow".into()))
}

/// Run StaticCpu for `max_instructions` and count hot VA pages.
pub fn profile_nixos_boot(
    manifest: &GuestManifest,
    max_instructions: u64,
) -> Result<(StaticCpu, NixosBootProfile), RelayError> {
    if max_instructions == 0 {
        return Err(RelayError::Failed(
            "AOT NixOS profile requires a positive instruction budget".into(),
        ));
    }
    let (mut cpu, _) = linux_boot::create_cpu(manifest)?;
    let mut page_hits: BTreeMap<u64, u64> = BTreeMap::new();
    let mut pcs: BTreeMap<u64, u64> = BTreeMap::new();
    for _ in 0..max_instructions {
        let pc = cpu.pc;
        *page_hits.entry(pc & !(PAGE - 1)).or_insert(0) += 1;
        *pcs.entry(pc).or_insert(0) += 1;
        cpu.step()?;
    }
    let profile = NixosBootProfile {
        instructions: max_instructions,
        unique_pages: page_hits.len(),
        unique_pcs: pcs.len(),
        page_hits,
    };
    Ok((cpu, profile))
}

/// Take the hottest pages that still map into the bundled kernel Image GPA.
///
/// When `cpu` is provided (end of a profile boot), guest VAs are walked through
/// the live MMU so post-MMU kernel text maps back onto the Image. Without a
/// CPU, only identity-mapped (pre-MMU) pages are accepted.
///
/// When `include_non_kernel` is set and `cpu` is present, remaining hot VAs
/// that peek successfully are signed from live RAM (module / early userspace
/// text). Those pages may not match a fresh boot under ASLR; use same-boot
/// measure for an upper bound.
pub fn hot_kernel_regions_from_image(
    manifest: &GuestManifest,
    profile: &NixosBootProfile,
    max_pages: usize,
    cpu: Option<&StaticCpu>,
) -> Result<BTreeMap<u64, Vec<u8>>, RelayError> {
    hot_regions_from_boot(manifest, profile, max_pages, cpu, false)
}

fn hot_regions_from_boot(
    manifest: &GuestManifest,
    profile: &NixosBootProfile,
    max_pages: usize,
    cpu: Option<&StaticCpu>,
    include_non_kernel: bool,
) -> Result<BTreeMap<u64, Vec<u8>>, RelayError> {
    if max_pages == 0 {
        return Err(RelayError::Failed(
            "AOT NixOS hot-page count must be positive".into(),
        ));
    }
    let header = read_image_header(&manifest.kernel)?;
    let kernel_pa = kernel_load_address(header.text_offset, header.flags)?;
    let kernel_end = kernel_pa
        .checked_add(header.image_size.max(manifest.kernel.bytes))
        .ok_or_else(|| RelayError::Failed("kernel image end overflow".into()))?;
    let image_bytes = fs::read(&manifest.kernel.path)
        .map_err(|error| RelayError::Failed(format!("cannot read guest kernel: {error}")))?;

    let mut ranked: Vec<(u64, u64)> = profile
        .page_hits
        .iter()
        .map(|(page, hits)| (*page, *hits))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut regions = BTreeMap::new();
    let mut covered_gpa: BTreeMap<u64, u64> = BTreeMap::new();
    // Scan the full hot ranking so longer boots can fill max_pages.
    for (page_va, _hits) in &ranked {
        if regions.len() >= max_pages {
            break;
        }
        let gpa = match cpu {
            Some(cpu) => match cpu.physical_for_aot(*page_va) {
                Ok(pa) => pa & !(PAGE - 1),
                Err(_) => continue,
            },
            None => *page_va,
        };
        if gpa < kernel_pa || gpa >= kernel_end {
            continue;
        }
        let bytes = if let Some(cpu) = cpu {
            // Prefer live guest bytes: relocatable Images are patched in RAM.
            // With nokaslr the relocated text is stable across boots.
            match peek_page_words(cpu, *page_va) {
                Some(live) => live,
                None => continue,
            }
        } else {
            match image_page_bytes(&image_bytes, kernel_pa, gpa) {
                Some(bytes) => bytes,
                None => continue,
            }
        };
        regions.insert(*page_va, bytes);
        covered_gpa.insert(gpa, *page_va);
    }

    // Linear kernel VA = GPA + delta from any hot MMU mapping. Seed cold
    // Image pages at that VA (and identity) so post-MMU measure hits engage.
    let linear_delta = covered_gpa.iter().find_map(|(gpa, va)| {
        if *va != *gpa {
            Some(va.wrapping_sub(*gpa))
        } else {
            None
        }
    });

    if let Some(cpu) = cpu {
        let mut gpa = kernel_pa;
        while gpa < kernel_end {
            if regions.len() >= max_pages {
                break;
            }
            let bytes = peek_page_words(cpu, gpa)
                .or_else(|| {
                    linear_delta.and_then(|delta| peek_page_words(cpu, gpa.wrapping_add(delta)))
                })
                .or_else(|| image_page_bytes(&image_bytes, kernel_pa, gpa));
            let Some(bytes) = bytes else {
                gpa = gpa.saturating_add(PAGE);
                continue;
            };
            if let Some(delta) = linear_delta {
                let va = gpa.wrapping_add(delta);
                if !regions.contains_key(&va) && regions.len() < max_pages {
                    // Prefer live bytes at the post-MMU VA when readable.
                    let page_bytes = peek_page_words(cpu, va).unwrap_or_else(|| bytes.clone());
                    regions.insert(va, page_bytes);
                    covered_gpa.entry(gpa).or_insert(va);
                }
            }
            if !regions.contains_key(&gpa) && regions.len() < max_pages {
                regions.insert(gpa, bytes);
                covered_gpa.entry(gpa).or_insert(gpa);
            }
            gpa = gpa.saturating_add(PAGE);
        }
    }

    if include_non_kernel {
        if let Some(cpu) = cpu {
            for (page_va, _hits) in &ranked {
                if regions.len() >= max_pages {
                    break;
                }
                if regions.contains_key(page_va) {
                    continue;
                }
                if let Some(bytes) = peek_page_words(cpu, *page_va) {
                    regions.insert(*page_va, bytes);
                }
            }
        }
    }

    if regions.is_empty() {
        return Err(RelayError::Failed(
            "no hot pages overlapped the bundled kernel Image; raise the profile budget".into(),
        ));
    }
    Ok(regions)
}

fn peek_page_words(cpu: &StaticCpu, page_va: u64) -> Option<Vec<u8>> {
    let mut live = Vec::with_capacity(PAGE as usize);
    for offset in (0..PAGE).step_by(4) {
        match cpu.peek_word(page_va.wrapping_add(offset)) {
            Ok(word) => live.extend_from_slice(&word.to_le_bytes()),
            Err(_) => return None,
        }
    }
    if live.len() < 4 {
        return None;
    }
    Some(live)
}

fn image_page_bytes(image_bytes: &[u8], kernel_pa: u64, gpa: u64) -> Option<Vec<u8>> {
    if gpa < kernel_pa {
        return None;
    }
    let offset = (gpa - kernel_pa) as usize;
    if offset >= image_bytes.len() {
        return None;
    }
    let end = (offset + PAGE as usize).min(image_bytes.len());
    let slice = &image_bytes[offset..end];
    if slice.len() < 4 {
        return None;
    }
    let usable = slice.len() - (slice.len() % 4);
    Some(slice[..usable].to_vec())
}

/// Profile, translate hot kernel pages, then measure AOT vs fallback on a fresh boot.
pub fn translate_nixos_hot_kernel(
    manifest: &GuestManifest,
    profile_instructions: u64,
    max_hot_pages: usize,
    measure_budget: u64,
) -> Result<NixosAotMeasure, RelayError> {
    translate_nixos_boot(
        manifest,
        profile_instructions,
        max_hot_pages,
        measure_budget,
        false,
        false,
    )
}

/// Same as [`translate_nixos_hot_kernel`], with optional non-kernel live pages
/// and optional same-boot measure (upper bound on the profile CPU).
pub fn translate_nixos_boot(
    manifest: &GuestManifest,
    profile_instructions: u64,
    max_hot_pages: usize,
    measure_budget: u64,
    include_non_kernel: bool,
    same_boot_measure: bool,
) -> Result<NixosAotMeasure, RelayError> {
    let (mut cpu, profile) = profile_nixos_boot(manifest, profile_instructions)?;
    let header = read_image_header(&manifest.kernel)?;
    let kernel_pa = kernel_load_address(header.text_offset, header.flags)?;
    let kernel_end = kernel_pa
        .checked_add(header.image_size.max(manifest.kernel.bytes))
        .ok_or_else(|| RelayError::Failed("kernel image end overflow".into()))?;

    let considered = profile.page_hits.len().min(max_hot_pages.max(1) * 4);
    let mut ranked: Vec<(u64, u64)> = profile
        .page_hits
        .iter()
        .map(|(page, hits)| (*page, *hits))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let candidate_pages = ranked.len().min(considered);

    let started = Instant::now();
    let regions = hot_regions_from_boot(
        manifest,
        &profile,
        max_hot_pages,
        Some(&cpu),
        include_non_kernel,
    )?;
    let mut kernel_pages = 0usize;
    let mut non_kernel_pages = 0usize;
    for page_va in regions.keys() {
        let gpa = cpu
            .physical_for_aot(*page_va)
            .map(|pa| pa & !(PAGE - 1))
            .unwrap_or(*page_va);
        if gpa >= kernel_pa && gpa < kernel_end {
            kernel_pages += 1;
        } else {
            non_kernel_pages += 1;
        }
    }
    let skipped = candidate_pages.saturating_sub(kernel_pages + non_kernel_pages);
    let entry = *regions.keys().next().unwrap();
    let image = translate_regions(entry, regions)?;
    let translator_ms = started.elapsed().as_millis();

    let stats: AotStats = if same_boot_measure {
        run_image(&mut cpu, &image, measure_budget)?
    } else {
        let (mut measure_cpu, _) = linux_boot::create_cpu(manifest)?;
        run_image(&mut measure_cpu, &image, measure_budget)?
    };
    let total = stats.aot_insns + stats.fallback_insns;
    let coverage = if total == 0 {
        0.0
    } else {
        (stats.aot_insns as f64) * 100.0 / (total as f64)
    };

    let measure = NixosAotMeasure {
        profile_instructions: profile.instructions,
        hot_pages: candidate_pages.min(max_hot_pages),
        kernel_pages,
        non_kernel_pages,
        skipped_non_kernel_pages: skipped,
        translator_ms,
        signed_text_bytes: image.signed_text_bytes,
        blocks: image.blocks.len(),
        measure_budget,
        aot_insns: stats.aot_insns,
        fallback_insns: stats.fallback_insns,
        aot_coverage_pct: coverage,
        nixos_kernel_translated: kernel_pages > 0,
        same_boot_measure,
    };
    let _ = image;
    Ok(measure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::GuestArtifact;
    use std::io::Write;

    fn tiny_relocatable_image(dir: &std::path::Path) -> GuestManifest {
        // Minimal ARM64 Image header + three instructions at file offset 0.
        let mut bytes = vec![0u8; 0x40];
        bytes[0..4].copy_from_slice(&0xd280_0100u32.to_le_bytes()); // MOVZ
        bytes[4..8].copy_from_slice(&0x9100_0400u32.to_le_bytes()); // ADD
        bytes[8..12].copy_from_slice(&0xd65f_03c0u32.to_le_bytes()); // RET
        bytes[0x10..0x18].copy_from_slice(&64u64.to_le_bytes()); // image_size
        bytes[0x18..0x20].copy_from_slice(&ARM64_IMAGE_RELOCATABLE.to_le_bytes());
        bytes[0x38..0x3c].copy_from_slice(&0x644d_5241u32.to_le_bytes());
        let kernel = dir.join("Image");
        fs::write(&kernel, &bytes).unwrap();
        let rootfs = dir.join("rootfs.img");
        let mut file = fs::File::create(&rootfs).unwrap();
        file.write_all(&[0u8; 4096]).unwrap();
        GuestManifest {
            version: GuestManifest::VERSION,
            page_size: 4096,
            memory_bytes: 16 * 1024 * 1024,
            kernel: GuestArtifact {
                path: kernel.display().to_string(),
                sha256: String::new(),
                bytes: bytes.len() as u64,
            },
            initrd: None,
            rootfs: GuestArtifact {
                path: rootfs.display().to_string(),
                sha256: String::new(),
                bytes: 4096,
            },
            command_line: "console=hvc0".into(),
            compatibility: Default::default(),
            signature: None,
        }
    }

    #[test]
    fn ranks_hot_pages_by_hit_count() {
        let mut profile = NixosBootProfile {
            instructions: 10,
            unique_pages: 2,
            unique_pcs: 2,
            page_hits: BTreeMap::new(),
        };
        profile.page_hits.insert(0x1000, 3);
        profile.page_hits.insert(0x2000, 9);
        let mut ranked: Vec<_> = profile.page_hits.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1));
        assert_eq!(*ranked[0].0, 0x2000);
    }

    #[test]
    fn kernel_region_extract_uses_image_bytes() {
        let dir = std::env::temp_dir().join("aot-nixos-extract");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let manifest = tiny_relocatable_image(&dir);
        let header = read_image_header(&manifest.kernel).unwrap();
        let kernel_pa = kernel_load_address(header.text_offset, header.flags).unwrap();
        let mut profile = NixosBootProfile {
            instructions: 1,
            unique_pages: 1,
            unique_pcs: 1,
            page_hits: BTreeMap::new(),
        };
        profile.page_hits.insert(kernel_pa, 100);
        let regions = hot_kernel_regions_from_image(&manifest, &profile, 1, None).unwrap();
        assert_eq!(regions.len(), 1);
        let bytes = regions.get(&kernel_pa).unwrap();
        assert!(bytes.len() >= 12);
        assert_eq!(&bytes[0..4], &0xd280_0100u32.to_le_bytes());
    }
}

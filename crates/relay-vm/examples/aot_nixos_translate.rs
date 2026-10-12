//! Offline-profile a NixOS guest boot and translate hot kernel pages.
//!
//! Feature: `aot-translate`. Does not ship in the store staticlib.
//!
//! usage: aot_nixos_translate GUEST_DIR PROFILE_INSNS HOT_PAGES MEASURE_BUDGET OUT.json
//!        [--non-kernel] [--same-boot]

use relay_core::{GuestArtifact, GuestManifest};
use relay_vm::translate_nixos_boot;
use std::{env, fs, path::Path};

fn resolve(directory: &Path, artifact: &mut GuestArtifact) {
    if Path::new(&artifact.path).is_relative() {
        artifact.path = directory.join(&artifact.path).display().to_string();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    let mut positional = Vec::new();
    let mut include_non_kernel = false;
    let mut same_boot = false;
    for arg in &args {
        match arg.as_str() {
            "--non-kernel" => include_non_kernel = true,
            "--same-boot" => same_boot = true,
            _ => positional.push(arg.clone()),
        }
    }
    if positional.len() != 5 {
        return Err(
            "usage: aot_nixos_translate GUEST_DIR PROFILE_INSNS HOT_PAGES MEASURE_BUDGET OUT.json [--non-kernel] [--same-boot]"
                .into(),
        );
    }
    let directory = Path::new(&positional[0]);
    let profile_insns: u64 = positional[1].parse()?;
    let hot_pages: usize = positional[2].parse()?;
    let measure_budget: u64 = positional[3].parse()?;
    let out = Path::new(&positional[4]);

    let mut manifest: GuestManifest =
        serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    resolve(directory, &mut manifest.kernel);
    if let Some(initrd) = &mut manifest.initrd {
        resolve(directory, initrd);
    }
    resolve(directory, &mut manifest.rootfs);

    let measure = translate_nixos_boot(
        &manifest,
        profile_insns,
        hot_pages,
        measure_budget,
        include_non_kernel,
        same_boot,
    )?;
    fs::write(out, serde_json::to_vec_pretty(&measure)?)?;
    eprintln!(
        "AOT NixOS: kernel_pages={} non_kernel_pages={} signed_text_bytes={} blocks={} aot_insns={} fallback_insns={} coverage={:.2}% translator_ms={} nixos_kernel_translated={} same_boot={}",
        measure.kernel_pages,
        measure.non_kernel_pages,
        measure.signed_text_bytes,
        measure.blocks,
        measure.aot_insns,
        measure.fallback_insns,
        measure.aot_coverage_pct,
        measure.translator_ms,
        measure.nixos_kernel_translated,
        measure.same_boot_measure
    );
    Ok(())
}

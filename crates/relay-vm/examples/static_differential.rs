use relay_core::{GuestArtifact, GuestManifest};
use relay_vm::differential::{first_divergence, read_trace, record_static_trace, TraceConfig};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    match arguments.next().as_deref().and_then(|value| value.to_str()) {
        Some("record") => {
            let guest_directory = path(&mut arguments, "GUEST_DIRECTORY")?;
            let output = path(&mut arguments, "OUTPUT.jsonl")?;
            let checkpoint_interval = number(&mut arguments, "CHECKPOINT_INTERVAL")?;
            let max_instructions = number(&mut arguments, "MAX_INSTRUCTIONS")?;
            reject_extra(arguments)?;
            let mut manifest: GuestManifest =
                serde_json::from_slice(&fs::read(guest_directory.join("manifest.json"))?)?;
            resolve(&guest_directory, &mut manifest.kernel);
            if let Some(initrd) = &mut manifest.initrd {
                resolve(&guest_directory, initrd);
            }
            resolve(&guest_directory, &mut manifest.rootfs);
            let count = record_static_trace(
                &manifest,
                TraceConfig {
                    checkpoint_interval,
                    max_instructions,
                },
                &output,
            )?;
            println!("wrote {count} checkpoints to {}", output.display());
        }
        Some("compare") => {
            let reference_path = path(&mut arguments, "REFERENCE.jsonl")?;
            let actual_path = path(&mut arguments, "ACTUAL.jsonl")?;
            reject_extra(arguments)?;
            let reference = read_trace(&reference_path)?;
            let actual = read_trace(&actual_path)?;
            if let Some(divergence) = first_divergence(&reference, &actual) {
                return Err(format!(
                    "first divergence at checkpoint {} reference={:?} actual={:?}: {}",
                    divergence.checkpoint_index,
                    divergence.reference_instructions,
                    divergence.actual_instructions,
                    divergence.reason
                )
                .into());
            }
            println!("traces match across {} checkpoints", reference.len());
        }
        _ => return Err(usage().into()),
    }
    Ok(())
}

fn path(
    arguments: &mut impl Iterator<Item = std::ffi::OsString>,
    name: &str,
) -> Result<PathBuf, String> {
    arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name}; {}", usage()))
}

fn number(
    arguments: &mut impl Iterator<Item = std::ffi::OsString>,
    name: &str,
) -> Result<u64, String> {
    arguments
        .next()
        .ok_or_else(|| format!("missing {name}; {}", usage()))?
        .to_string_lossy()
        .parse()
        .map_err(|_| format!("invalid {name}; {}", usage()))
}

fn reject_extra(mut arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<(), String> {
    if arguments.next().is_some() {
        Err(usage().into())
    } else {
        Ok(())
    }
}

fn usage() -> &'static str {
    "usage: static_differential record GUEST_DIRECTORY OUTPUT.jsonl CHECKPOINT_INTERVAL MAX_INSTRUCTIONS | static_differential compare REFERENCE.jsonl ACTUAL.jsonl"
}

fn resolve(directory: &std::path::Path, artifact: &mut GuestArtifact) {
    let path = std::path::Path::new(&artifact.path);
    if path.is_relative() {
        artifact.path = directory.join(path).display().to_string();
    }
}

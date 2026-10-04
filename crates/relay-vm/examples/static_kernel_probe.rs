use relay_core::{GuestArtifact, GuestManifest};
use relay_vm::kernel_probe::{record, Config};
use std::{env, fs, io::BufWriter, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: static_kernel_probe GUEST_DIRECTORY CONFIG.json OUTPUT.jsonl".into());
    }
    let directory = Path::new(&args[0]);
    let mut manifest: GuestManifest =
        serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    fn resolve(directory: &Path, artifact: &mut GuestArtifact) {
        if Path::new(&artifact.path).is_relative() {
            artifact.path = directory.join(&artifact.path).display().to_string();
        }
    }
    resolve(directory, &mut manifest.kernel);
    if let Some(initrd) = &mut manifest.initrd {
        resolve(directory, initrd);
    }
    resolve(directory, &mut manifest.rootfs);
    let config: Config = serde_json::from_slice(&fs::read(&args[1])?)?;
    // Never silently overwrite earlier evidence.
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    record(&manifest, config, &mut BufWriter::new(file))
}

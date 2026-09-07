//! Shared CLI entry used by `bin/wpm` and `wpm_main`.

use anyhow::{bail, Context, Result};

use crate::store::PackageStore;
use crate::{DEFAULT_REGISTRY, REGISTRY_ENV, STORE_ENV};

pub fn cli_run(args: &[String]) -> Result<i32> {
    let argv0 = args.first().map(|s| s.as_str()).unwrap_or("wpm");
    let rest: Vec<&str> = args.iter().skip(1).map(|s| s.as_str()).collect();
    let cmd = rest.first().copied().unwrap_or("help");

    match cmd {
        "help" | "-h" | "--help" => {
            print_help(argv0);
            Ok(0)
        }
        "list" => {
            let store = PackageStore::open_default()?;
            let mut pkgs = store.list()?;
            pkgs.sort_by(|a, b| a.name.cmp(&b.name));
            if pkgs.is_empty() {
                println!("(no packages installed)");
            } else {
                println!("{:<24} {:<12} {:<8} {}", "NAME", "VERSION", "SOURCE", "DIGEST");
                for p in pkgs {
                    let dig = p.digest.get(..19).unwrap_or(&p.digest);
                    println!(
                        "{:<24} {:<12} {:<8} {}…",
                        p.name, p.version, p.source, dig
                    );
                }
            }
            Ok(0)
        }
        "path" => {
            let name = rest.get(1).context("usage: wpm path <name>")?;
            let store = PackageStore::open_default()?;
            println!("{}", store.resolve_wasm(name)?.display());
            Ok(0)
        }
        "remove" | "uninstall" => {
            let name = rest.get(1).context("usage: wpm remove <name>")?;
            let store = PackageStore::open_default()?;
            store.remove(name)?;
            println!("removed {name}");
            Ok(0)
        }
        "install" => {
            let target = rest.get(1).context("usage: wpm install <name|./file.wasm>")?;
            let store = PackageStore::open_default()?;
            if target.contains('/') || target.ends_with(".wasm") || target.starts_with('.') {
                let path = std::path::Path::new(target);
                let name = rest.iter().position(|a| *a == "--name").and_then(|i| rest.get(i + 1));
                let ver = rest
                    .iter()
                    .position(|a| *a == "--version")
                    .and_then(|i| rest.get(i + 1))
                    .unwrap_or(&"0.0.0");
                let pkg = store.install_local(path, name.copied(), ver)?;
                println!(
                    "installed {} {} ({})",
                    pkg.name, pkg.version, pkg.digest
                );
                println!("run: wasm {}   or   wasm $(wpm path {})", pkg.name, pkg.name);
                Ok(0)
            } else {
                #[cfg(feature = "registry")]
                {
                    let (name, ver) = split_name_ver(target);
                    let client = crate::registry::RegistryClient::from_env_or_default()?;
                    client.install(&store, name, ver)?;
                    println!("installed {target} from {}", client.base());
                    println!("run: wasm {name}");
                    Ok(0)
                }
                #[cfg(not(feature = "registry"))]
                {
                    bail!("registry support not built; use: wpm install ./file.wasm");
                }
            }
        }
        "search" => {
            #[cfg(feature = "registry")]
            {
                let q = rest.get(1).copied().unwrap_or("");
                let client = crate::registry::RegistryClient::from_env_or_default()?;
                let hits = client.search(q)?;
                if hits.is_empty() {
                    println!("(no matches - is {DEFAULT_REGISTRY}/index.json published?)");
                } else {
                    println!("{:<24} {:<12} {}", "NAME", "VERSION", "SUMMARY");
                    for p in hits {
                        println!(
                            "{:<24} {:<12} {}",
                            p.name,
                            p.version,
                            p.summary.unwrap_or_default()
                        );
                    }
                }
                Ok(0)
            }
            #[cfg(not(feature = "registry"))]
            {
                bail!("registry support not built");
            }
        }
        "show" => {
            let name = rest.get(1).context("usage: wpm show <name>")?;
            let store = PackageStore::open_default()?;
            if let Some(p) = store.get(name)? {
                println!("name:    {}", p.name);
                println!("version: {}", p.version);
                println!("digest:  {}", p.digest);
                println!("wasi:    {}", p.wasi);
                println!("source:  {}", p.source);
                println!("path:    {}", store.resolve_wasm(name)?.display());
                return Ok(0);
            }
            #[cfg(feature = "registry")]
            {
                let client = crate::registry::RegistryClient::from_env_or_default()?;
                let hits = client.search(name)?;
                let hit = hits.into_iter().find(|p| p.name == *name);
                if let Some(p) = hit {
                    println!("name:    {} (registry)", p.name);
                    println!("version: {}", p.version);
                    println!("digest:  {}", p.digest);
                    println!("url:     {}/{}", client.base(), p.url);
                    if let Some(s) = p.summary {
                        println!("summary: {s}");
                    }
                    return Ok(0);
                }
            }
            bail!("package not found: {name}");
        }
        other => {
            bail!("unknown command: {other} (try: wpm help)");
        }
    }
}

#[cfg(feature = "registry")]
fn split_name_ver(spec: &str) -> (&str, Option<&str>) {
    if let Some((n, v)) = spec.split_once('@') {
        (n, Some(v))
    } else {
        (spec, None)
    }
}

fn print_help(argv0: &str) {
    println!(
        "wpm - Wawona Runtime package manager (WASI .wasm)\n\
         \n\
         Usage: {argv0} <command>\n\
         \n\
           list                         Installed packages\n\
           install ./file.wasm          Register a local .wasm (Files.app / sideload)\n\
           install <name>[@ver]         Install from Mode A registry\n\
           remove <name>                Uninstall\n\
           path <name>                  Print blob path\n\
           show <name>                  Metadata\n\
           search [query]               Search {DEFAULT_REGISTRY}\n\
           help                         This text\n\
         \n\
         Store:  ${STORE_ENV} or ~/Library/Application Support/Wawona/wasm-packages\n\
         Registry (Mode A only): ${REGISTRY_ENV} default {DEFAULT_REGISTRY}\n\
         Jailbreak .deb APT is a different channel - never used by wpm.\n\
         Run packages:  wasm <name>   (Runtime resolves installed names)\n"
    );
}

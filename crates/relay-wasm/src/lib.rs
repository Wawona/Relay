//! Mode A WASI. Prepare validates bytecode; start executes via the linked
//! `wawona_wasm_run` symbol (same process as `libwawona_wasm.a`) or a host
//! `wasm` / `WAWONA_WASM` binary. Keeps `import/wasm` as its own workspace.

use relay_core::{resolve_backend, RelayBackend, RelayError, RelayKind, RelaySpec};
use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmExecutionPlan {
    pub backend: RelayBackend,
    pub module: Option<PathBuf>,
    pub package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmHandle {
    pub id: String,
    pub backend: RelayBackend,
    pub module: Option<PathBuf>,
}

enum Guest {
    Thread(JoinHandle<Result<i32, String>>),
    Process(Child),
}

struct LiveWasm {
    backend: RelayBackend,
    guest: Option<Guest>,
    exit_code: Option<i32>,
}

fn live() -> &'static Mutex<HashMap<String, LiveWasm>> {
    static LIVE: OnceLock<Mutex<HashMap<String, LiveWasm>>> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

const MAX_MODULE_BYTES: u64 = 512 * 1024 * 1024;

type WasmRunFn = unsafe extern "C" fn(libc::c_int, *const *const libc::c_char) -> libc::c_int;

pub fn resolve(spec: &RelaySpec) -> Result<RelayBackend, RelayError> {
    if spec.kind != RelayKind::Wasm {
        return Err(RelayError::Failed(
            "relay-wasm only handles kind=wasm".into(),
        ));
    }
    resolve_backend(spec)
}

/// Validate an explicit module or resolve a `wpm` package name to its blob.
pub fn prepare(spec: &RelaySpec) -> Result<WasmExecutionPlan, RelayError> {
    let backend = resolve(spec)?;
    let image = spec
        .image
        .as_deref()
        .map(str::trim)
        .filter(|image| !image.is_empty());
    let mut package = None;
    let module = match image {
        Some(image) if image.contains('/') || image.ends_with(".wasm") => {
            Some(validate_module_path(Path::new(image))?)
        }
        Some(image) => {
            validate_package_name(image)?;
            package = Some(image.to_string());
            match resolve_wpm_package(image) {
                Ok(path) => Some(validate_module_path(&path)?),
                Err(RelayError::Failed(_)) => None,
                Err(other) => return Err(other),
            }
        }
        None => None,
    };
    Ok(WasmExecutionPlan {
        backend,
        module,
        package,
    })
}

/// Start WASI P1/P2 execution.
pub fn start(spec: &RelaySpec) -> Result<WasmHandle, RelayError> {
    let plan = prepare(spec)?;
    let module = match (&plan.module, &plan.package) {
        (Some(path), _) => path.clone(),
        (None, Some(package)) => {
            return Err(RelayError::Failed(format!(
                "WASM package `{package}` is not installed; run `wpm install {package}` (Mode A /wasm/v1 only)"
            )));
        }
        (None, None) => {
            return Err(RelayError::Failed(
                "Relay wasm start requires a .wasm module path or package name".into(),
            ));
        }
    };

    let id = format!("relay-wasm-{}-{}", std::process::id(), next_id());
    let backend = plan.backend;
    let guest = launch_guest(&module)?;

    live()
        .lock()
        .map_err(|_| RelayError::Failed("relay-wasm session lock poisoned".into()))?
        .insert(
            id.clone(),
            LiveWasm {
                backend,
                guest: Some(guest),
                exit_code: None,
            },
        );
    Ok(WasmHandle {
        id,
        backend,
        module: Some(module),
    })
}

pub fn stop(id: &str) -> Result<(), RelayError> {
    let mut session = live()
        .lock()
        .map_err(|_| RelayError::Failed("relay-wasm session lock poisoned".into()))?
        .remove(id)
        .ok_or_else(|| RelayError::Failed(format!("unknown Relay wasm handle: {id}")))?;
    match session.guest.take() {
        Some(Guest::Thread(join)) => match join.join() {
            Ok(Ok(code)) => session.exit_code = Some(code),
            Ok(Err(error)) => {
                return Err(RelayError::Failed(format!(
                    "Relay wasm guest failed: {error}"
                )));
            }
            Err(_) => {
                return Err(RelayError::Failed(
                    "Relay wasm guest thread panicked".into(),
                ));
            }
        },
        Some(Guest::Process(mut child)) => {
            let _ = child.kill();
            let status = child.wait().map_err(|error| {
                RelayError::Failed(format!("cannot wait for Relay wasm process: {error}"))
            })?;
            session.exit_code = status.code();
        }
        None => {}
    }
    let _ = session;
    Ok(())
}

pub fn status(id: &str) -> Result<&'static str, RelayError> {
    let mut guard = live()
        .lock()
        .map_err(|_| RelayError::Failed("relay-wasm session lock poisoned".into()))?;
    let session = guard
        .get_mut(id)
        .ok_or_else(|| RelayError::Failed(format!("unknown Relay wasm handle: {id}")))?;
    match session.guest.take() {
        Some(Guest::Thread(join)) => {
            if join.is_finished() {
                match join.join() {
                    Ok(Ok(code)) => {
                        session.exit_code = Some(code);
                        return Ok("exited");
                    }
                    Ok(Err(error)) => {
                        return Err(RelayError::Failed(format!(
                            "Relay wasm guest failed: {error}"
                        )));
                    }
                    Err(_) => {
                        return Err(RelayError::Failed(
                            "Relay wasm guest thread panicked".into(),
                        ));
                    }
                }
            }
            session.guest = Some(Guest::Thread(join));
            Ok("running")
        }
        Some(Guest::Process(mut child)) => match child.try_wait() {
            Ok(Some(status)) => {
                session.exit_code = status.code();
                Ok("exited")
            }
            Ok(None) => {
                session.guest = Some(Guest::Process(child));
                Ok("running")
            }
            Err(error) => Err(RelayError::Failed(format!(
                "cannot poll Relay wasm process: {error}"
            ))),
        },
        None => {
            if session.exit_code.is_some() {
                Ok("exited")
            } else {
                Ok("running")
            }
        }
    }
}

fn launch_guest(module: &Path) -> Result<Guest, RelayError> {
    if let Some(run) = wasm_run_symbol() {
        let module = module.to_path_buf();
        return Ok(Guest::Thread(thread::spawn(move || {
            let module_c = CString::new(module.display().to_string())
                .map_err(|error| format!("WASM path has interior NUL: {error}"))?;
            let argv = [module_c.as_ptr()];
            let code = unsafe { run(1, argv.as_ptr()) };
            Ok(code)
        })));
    }
    let binary = find_wasm_binary().ok_or_else(|| {
        RelayError::Planned(
            "Relay wasm execute needs linked wawona_wasm_run or WAWONA_WASM / wasm on PATH"
                .into(),
        )
    })?;
    let child = Command::new(&binary)
        .arg(module)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            RelayError::Failed(format!(
                "cannot spawn {} for Relay wasm: {error}",
                binary.display()
            ))
        })?;
    Ok(Guest::Process(child))
}

fn wasm_run_symbol() -> Option<WasmRunFn> {
    #[cfg(unix)]
    {
        unsafe {
            let name = CString::new("wawona_wasm_run").ok()?;
            let ptr = libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(std::mem::transmute::<*mut libc::c_void, WasmRunFn>(ptr))
            }
        }
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn find_wasm_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("WAWONA_WASM") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("wasm");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn validate_module_path(path: &Path) -> Result<PathBuf, RelayError> {
    let metadata = fs::metadata(path)
        .map_err(|error| RelayError::Failed(format!("cannot stat WASM module: {error}")))?;
    if !metadata.is_file() || metadata.len() < 8 || metadata.len() > MAX_MODULE_BYTES {
        return Err(RelayError::Failed("WASM module is not a valid file".into()));
    }
    let bytes = fs::read(path)
        .map_err(|error| RelayError::Failed(format!("cannot read WASM module: {error}")))?;
    if bytes.get(..4) != Some(b"\0asm") {
        return Err(RelayError::Failed("WASM module has invalid magic".into()));
    }
    wasmparser::Validator::new()
        .validate_all(&bytes)
        .map_err(|error| RelayError::Failed(format!("invalid WASM module: {error}")))?;
    Ok(path.to_path_buf())
}

fn resolve_wpm_package(name: &str) -> Result<PathBuf, RelayError> {
    let store = wpm::PackageStore::open_default().map_err(|error| {
        RelayError::Failed(format!("cannot open wpm package store: {error}"))
    })?;
    store.resolve_wasm(name).map_err(|error| {
        RelayError::Failed(format!(
            "WASM package `{name}` is not installed in wpm store: {error}"
        ))
    })
}

fn validate_package_name(name: &str) -> Result<(), RelayError> {
    if name.len() > 128
        || name.ends_with(".deb")
        || name.contains("://")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(RelayError::Failed(
            "WASM package name is not valid for /wasm/v1".into(),
        ));
    }
    Ok(())
}

fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::{ArtifactClass, RelayPlatform};
    use std::sync::{Mutex, OnceLock};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);
    static STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn with_temp_store<R>(f: impl FnOnce(&Path) -> R) -> R {
        let _guard = STORE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = tempfile::tempdir().unwrap();
        std::env::set_var(wpm::STORE_ENV, root.path());
        let out = f(root.path());
        std::env::remove_var(wpm::STORE_ENV);
        out
    }

    fn spec(platform: RelayPlatform, image: Option<String>) -> RelaySpec {
        RelaySpec {
            kind: RelayKind::Wasm,
            platform,
            artifact: ArtifactClass::ModeA,
            machine_id: None,
            image,
            memory_mb: None,
            guest_page_size: None,
            guest: None,
            resources: None,
            ios_hv_host: None,
        }
    }

    #[test]
    fn apple_mobile_uses_pulley_desktop_uses_cranelift_android_mode_a_pulley() {
        for platform in [
            RelayPlatform::Ios,
            RelayPlatform::Ipados,
            RelayPlatform::Tvos,
            RelayPlatform::Watchos,
            RelayPlatform::Visionos,
        ] {
            assert_eq!(
                prepare(&spec(platform, None)).unwrap().backend,
                RelayBackend::WasmPulley
            );
        }
        assert_eq!(
            prepare(&spec(RelayPlatform::Android, None))
                .unwrap()
                .backend,
            RelayBackend::WasmPulley
        );
        for platform in [RelayPlatform::Macos, RelayPlatform::Linux] {
            assert_eq!(
                prepare(&spec(platform, None)).unwrap().backend,
                RelayBackend::WasmCranelift
            );
        }
    }

    #[test]
    fn validates_explicit_module_before_execution() {
        let path = std::env::temp_dir().join(format!(
            "relay-wasm-{}.wasm",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, b"\0asm\x01\0\0\0").unwrap();
        let plan = prepare(&spec(
            RelayPlatform::Macos,
            Some(path.display().to_string()),
        ))
        .unwrap();
        assert_eq!(plan.module.as_deref(), Some(path.as_path()));
        assert_eq!(plan.package, None);
        fs::write(&path, b"\0asm\x02\0\0\0").unwrap();
        assert!(prepare(&spec(
            RelayPlatform::Macos,
            Some(path.display().to_string())
        ))
        .is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn bare_wpm_package_names_stay_named_until_installed() {
        with_temp_store(|_| {
            let plan = prepare(&spec(RelayPlatform::Ios, Some("hello-wasi-gui".into()))).unwrap();
            assert_eq!(plan.module, None);
            assert_eq!(plan.package.as_deref(), Some("hello-wasi-gui"));
            assert!(prepare(&spec(
                RelayPlatform::Ios,
                Some("jailbreak-package.deb".into())
            ))
            .is_err());
        });
    }

    #[test]
    fn start_rejects_missing_wpm_package_with_install_hint() {
        with_temp_store(|_| {
            let err = start(&spec(RelayPlatform::Macos, Some("hello-wasi-gui".into()))).unwrap_err();
            let RelayError::Failed(message) = err else {
                panic!("expected Failed, got {err:?}");
            };
            assert!(message.contains("wpm install hello-wasi-gui"));
        });
    }

    #[test]
    fn prepare_resolves_installed_wpm_package() {
        with_temp_store(|root| {
            let wasm = root.join("fixture.wasm");
            fs::write(&wasm, b"\0asm\x01\0\0\0").unwrap();
            let store = wpm::PackageStore::open(root).unwrap();
            store
                .install_local(&wasm, Some("relay-fixture"), "0.0.1")
                .unwrap();
            let plan = prepare(&spec(
                RelayPlatform::Macos,
                Some("relay-fixture".into()),
            ))
            .unwrap();
            assert_eq!(plan.package.as_deref(), Some("relay-fixture"));
            let module = plan.module.expect("resolved module");
            assert!(module.is_file());
            assert!(module.ends_with("component.wasm"));
        });
    }

    #[test]
    fn start_without_executor_is_planned_for_tiny_module() {
        let path = std::env::temp_dir().join(format!(
            "relay-wasm-exec-{}.wasm",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, b"\0asm\x01\0\0\0").unwrap();
        // Clear PATH lookup noise: if neither symbol nor binary is present,
        // start must fail closed as Planned (not crash).
        let result = start(&spec(
            RelayPlatform::Macos,
            Some(path.display().to_string()),
        ));
        match result {
            Ok(handle) => {
                let _ = stop(&handle.id);
            }
            Err(RelayError::Planned(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
        let _ = fs::remove_file(path);
    }
}

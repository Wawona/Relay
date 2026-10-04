//! C ABI. Keep `wawona_wasm_*` / `wwn_vm_*` in their own archives for overlap.

use relay_core::{resolve_backend, RelayBackend, RelayKind, RelaySpec};
use relay_oci::prepare_bundle;
use relay_vm::{
    console_log as vm_console_log, generations_json as vm_generations_json, start as vm_start,
    status as vm_status, stop as vm_stop, RelayHandle, RelayVmStatus,
};
use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::{Mutex, OnceLock};

static HANDLES: Mutex<Vec<RelayHandle>> = Mutex::new(Vec::new());

fn wasm_handles() -> &'static Mutex<HashSet<String>> {
    static WASM_HANDLES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    WASM_HANDLES.get_or_init(|| Mutex::new(HashSet::new()))
}

const OK: c_int = 0;
const ERR_FORBIDDEN: c_int = -1;
const ERR_PLANNED: c_int = -2;
const ERR_FAIL: c_int = -3;
const ERR_ARG: c_int = -4;

fn cstr<'a>(p: *const c_char) -> Result<&'a str, c_int> {
    if p.is_null() {
        return Err(ERR_ARG);
    }
    unsafe { CStr::from_ptr(p) }.to_str().map_err(|_| ERR_ARG)
}

fn give(out: *mut *mut c_char, s: &str) -> c_int {
    if out.is_null() {
        return ERR_ARG;
    }
    match CString::new(s) {
        Ok(c) => {
            unsafe { *out = c.into_raw() };
            OK
        }
        Err(_) => ERR_FAIL,
    }
}

fn map_err(e: relay_core::RelayError) -> c_int {
    match e {
        relay_core::RelayError::Forbidden(_) => ERR_FORBIDDEN,
        relay_core::RelayError::Planned(_) => ERR_PLANNED,
        relay_core::RelayError::Failed(_) => ERR_FAIL,
    }
}

fn wasmer_webkit_start(module: &str, handle_out: *mut *mut c_char) -> c_int {
    #[cfg(unix)]
    {
        use std::os::raw::c_void;
        unsafe extern "C" {
            fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        }
        #[cfg(target_os = "macos")]
        const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
        #[cfg(not(target_os = "macos"))]
        const RTLD_DEFAULT: *mut c_void = std::ptr::null_mut();
        type StartFn = unsafe extern "C" fn(*const c_char, *mut *mut c_char) -> c_int;
        let symbol = match CString::new("wwn_wasmer_webkit_start") {
            Ok(s) => s,
            Err(_) => return ERR_ARG,
        };
        let ptr = unsafe { dlsym(RTLD_DEFAULT, symbol.as_ptr()) };
        if ptr.is_null() {
            let _ = give(
                handle_out,
                "Wasmer WASIX WebKit host is not linked; iOS 13-26 stays on Pulley",
            );
            return ERR_PLANNED;
        }
        let start: StartFn = unsafe { std::mem::transmute(ptr) };
        let module = match CString::new(module) {
            Ok(s) => s,
            Err(_) => return ERR_ARG,
        };
        return unsafe { start(module.as_ptr(), handle_out) };
    }
    #[cfg(not(unix))]
    {
        let _ = (module, handle_out);
        ERR_PLANNED
    }
}

fn parse_spec(json: *const c_char) -> Result<RelaySpec, c_int> {
    let s = cstr(json)?;
    serde_json::from_str(s).map_err(|_| ERR_ARG)
}

#[no_mangle]
/// # Safety
/// Pointer arguments must be valid for the documented C ABI operation.
pub unsafe extern "C" fn relay_resolve_backend(
    spec_json: *const c_char,
    backend_out: *mut *mut c_char,
) -> c_int {
    let spec = match parse_spec(spec_json) {
        Ok(s) => s,
        Err(c) => return c,
    };
    match resolve_backend(&spec) {
        Ok(b) => give(backend_out, b.as_str()),
        Err(e) => {
            let _ = give(backend_out, &e.to_string());
            map_err(e)
        }
    }
}

#[no_mangle]
/// # Safety
/// Pointer arguments must be valid for the documented C ABI operation.
pub unsafe extern "C" fn relay_start(
    spec_json: *const c_char,
    handle_out: *mut *mut c_char,
) -> c_int {
    let mut spec = match parse_spec(spec_json) {
        Ok(s) => s,
        Err(c) => return c,
    };
    if spec.kind == RelayKind::Wasm {
        if matches!(resolve_backend(&spec), Ok(RelayBackend::WasmWasmerWebKit)) {
            return wasmer_webkit_start(spec.image.as_deref().unwrap_or(""), handle_out);
        }
        match relay_wasm::start(&spec) {
            Ok(handle) => {
                if let Ok(mut g) = wasm_handles().lock() {
                    g.insert(handle.id.clone());
                }
                return give(handle_out, &handle.id);
            }
            Err(e) => {
                let _ = give(handle_out, &e.to_string());
                return map_err(e);
            }
        }
    }
    match prepare_bundle(&spec) {
        Ok(Some(bundle)) => spec.image = Some(bundle),
        Ok(None) => {}
        Err(e) => {
            let _ = give(handle_out, &e.to_string());
            return map_err(e);
        }
    }
    match vm_start(&spec) {
        Ok(h) => {
            let id = h.id.clone();
            if let Ok(mut g) = HANDLES.lock() {
                g.push(h);
            }
            give(handle_out, &id)
        }
        Err(e) => {
            let _ = give(handle_out, &e.to_string());
            map_err(e)
        }
    }
}

#[no_mangle]
/// # Safety
/// `handle` must be null or point to a valid NUL-terminated string.
pub unsafe extern "C" fn relay_stop(handle: *const c_char) -> c_int {
    let id = match cstr(handle) {
        Ok(s) => s,
        Err(c) => return c,
    };
    if let Ok(mut g) = wasm_handles().lock() {
        if g.remove(id) {
            return match relay_wasm::stop(id) {
                Ok(()) => OK,
                Err(e) => map_err(e),
            };
        }
    }
    match vm_stop(id) {
        Ok(()) => {
            if let Ok(mut g) = HANDLES.lock() {
                g.retain(|h| h.id != id);
            }
            OK
        }
        Err(e) => map_err(e),
    }
}

#[no_mangle]
/// # Safety
/// Handle must be a valid NUL-terminated string. Entry must remain callable
/// until stop succeeds, retain no borrowed descriptor, and return on channel
/// closure. Entry may duplicate its descriptor for the synchronous call.
pub unsafe extern "C" fn relay_start_host_waypipe(
    handle: *const c_char,
    entry: Option<relay_vm::HostWaypipeEntry>,
) -> c_int {
    let id = match cstr(handle) {
        Ok(id) => id,
        Err(error) => return error,
    };
    let Some(entry) = entry else {
        return ERR_ARG;
    };
    // SAFETY: C ABI caller supplies the contracted native entry.
    match unsafe { relay_vm::start_host_waypipe(id, entry) } {
        Ok(()) => OK,
        Err(error) => map_err(error),
    }
}

#[no_mangle]
/// # Safety
/// Pointer arguments must be valid for the documented C ABI operation.
pub unsafe extern "C" fn relay_wayland_endpoint(
    handle: *const c_char,
    endpoint_out: *mut *mut c_char,
) -> c_int {
    let id = match cstr(handle) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let g = match HANDLES.lock() {
        Ok(g) => g,
        Err(_) => return ERR_FAIL,
    };
    match g.iter().find(|h| h.id == id) {
        Some(h) => match &h.wayland_endpoint {
            Some(ep) => give(endpoint_out, ep),
            None => {
                let _ = give(endpoint_out, "");
                ERR_PLANNED
            }
        },
        None => ERR_ARG,
    }
}

#[no_mangle]
/// # Safety
/// Pointer arguments must be valid for the documented C ABI operation.
pub unsafe extern "C" fn relay_status(
    handle: *const c_char,
    status_out: *mut *mut c_char,
) -> c_int {
    let id = match cstr(handle) {
        Ok(s) => s,
        Err(c) => return c,
    };
    if let Ok(g) = wasm_handles().lock() {
        if g.contains(id) {
            return match relay_wasm::status(id) {
                Ok(status) => give(status_out, status),
                Err(error) => {
                    let _ = give(status_out, &error.to_string());
                    map_err(error)
                }
            };
        }
    }
    match vm_status(id) {
        Ok(RelayVmStatus::Running) => give(status_out, "running"),
        Ok(RelayVmStatus::Exited) => give(status_out, "exited"),
        Err(error) => {
            let _ = give(status_out, &error.to_string());
            map_err(error)
        }
    }
}

#[no_mangle]
/// # Safety
/// Pointer arguments must be valid for their declared capacities.
pub unsafe extern "C" fn relay_copy_log(
    handle: *const c_char,
    bytes: *mut u8,
    capacity: usize,
    length_out: *mut usize,
) -> c_int {
    let id = match cstr(handle) {
        Ok(s) => s,
        Err(c) => return c,
    };
    if length_out.is_null() {
        return ERR_ARG;
    }
    let log = match vm_console_log(id) {
        Ok(log) => log,
        Err(error) => return map_err(error),
    };
    unsafe { *length_out = log.len() };
    if bytes.is_null() {
        return OK;
    }
    if capacity < log.len() {
        return ERR_FAIL;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(log.as_ptr(), bytes, log.len());
    }
    OK
}

#[no_mangle]
/// # Safety
/// Pointer arguments must be valid for their declared capacities.
pub unsafe extern "C" fn relay_copy_frame(
    handle: *const c_char,
    rgba: *mut u8,
    len: usize,
    width_out: *mut u32,
    height_out: *mut u32,
) -> c_int {
    let id = match cstr(handle) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let g = match HANDLES.lock() {
        Ok(g) => g,
        Err(_) => return ERR_FAIL,
    };
    let h = match g.iter().find(|h| h.id == id) {
        Some(h) => h,
        None => return ERR_ARG,
    };
    let (w, ht) = (h.frame_width, h.frame_height);
    if !width_out.is_null() {
        unsafe { *width_out = w };
    }
    if !height_out.is_null() {
        unsafe { *height_out = ht };
    }
    let Some(frame) = h.frame.as_ref() else {
        return ERR_PLANNED;
    };
    if rgba.is_null() {
        return OK;
    }
    if len < frame.len() {
        return ERR_FAIL;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(frame.as_ptr(), rgba, frame.len());
    }
    OK
}

#[no_mangle]
/// # Safety
/// Pointer arguments must be valid for the documented C ABI operation.
pub unsafe extern "C" fn relay_probe_ios_hv(
    spec_json: *const c_char,
    json_out: *mut *mut c_char,
) -> c_int {
    let spec = match parse_spec(spec_json) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let live = relay_core::live_ios_hv_host();
    let host = spec.ios_hv_host.as_ref().or(live.as_ref());
    let probe = relay_core::probe_ios_hv(spec.artifact, host);
    match serde_json::to_string(&probe) {
        Ok(s) => give(json_out, &s),
        Err(_) => ERR_FAIL,
    }
}

#[no_mangle]
/// # Safety
/// `s` must be null or a pointer returned by this library exactly once.
pub unsafe extern "C" fn relay_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_start(json: &str) -> (c_int, String) {
        let json = CString::new(json).unwrap();
        let mut output = std::ptr::null_mut();
        let code = unsafe { relay_start(json.as_ptr(), &mut output) };
        let text = if output.is_null() {
            String::new()
        } else {
            let text = unsafe { CStr::from_ptr(output) }
                .to_string_lossy()
                .into_owned();
            unsafe { relay_string_free(output) };
            text
        };
        (code, text)
    }

    #[test]
    fn wasm_package_start_requires_wpm_install() {
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("WAWONA_WASM_STORE", root.path());
        let (code, output) = call_start(
            r#"{"kind":"wasm","platform":"macos","artifact":"mode_a","image":"hello-wasi-gui"}"#,
        );
        std::env::remove_var("WAWONA_WASM_STORE");
        assert_eq!(code, ERR_FAIL);
        assert!(output.contains("wpm install hello-wasi-gui"));
    }

    #[test]
    fn macos_vm_requires_explicit_runtime_resources() {
        let (code, output) = call_start(r#"{"kind":"vm","platform":"macos","artifact":"mode_a"}"#);
        assert_eq!(code, ERR_FAIL);
        assert!(output.contains("runtime resources"));
    }

    #[test]
    fn mode_b_window_device_probe_is_supported() {
        let json = CString::new(
            r#"{"kind":"vm","platform":"ios","artifact":"mode_b","ios_hv_host":{"os":{"major":16,"minor":3,"patch":1},"hw_machine":"iPhone15,2","kernel_hv":true,"has_private_hypervisor_entitlement":true}}"#,
        )
        .unwrap();
        let mut output = std::ptr::null_mut();
        let code = unsafe { relay_probe_ios_hv(json.as_ptr(), &mut output) };
        assert_eq!(code, OK);
        let text = unsafe { CStr::from_ptr(output) }
            .to_string_lossy()
            .into_owned();
        unsafe { relay_string_free(output) };
        assert!(text.contains("\"supported\":true"));
        let mut backend = std::ptr::null_mut();
        let code = unsafe { relay_resolve_backend(json.as_ptr(), &mut backend) };
        assert_eq!(code, OK);
        let name = unsafe { CStr::from_ptr(backend) }
            .to_string_lossy()
            .into_owned();
        unsafe { relay_string_free(backend) };
        assert_eq!(name, "ios-hv");
    }
}

#[no_mangle]
/// # Safety
/// `name` and optional `source` are NUL-terminated UTF-8. `json_out` must be
/// writable; free the returned string with `relay_string_free`.
pub unsafe extern "C" fn relay_nix_editor(
    name: *const c_char,
    source: *const c_char,
    json_out: *mut *mut c_char,
) -> c_int {
    let name = match cstr(name) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let default = match relay_core::nix_editor::default_file(name) {
        Some(s) => s,
        None => return ERR_ARG,
    };
    let source = if source.is_null() {
        default
    } else {
        match cstr(source) {
            Ok(s) => s,
            Err(e) => return e,
        }
    };
    if source.len() > 1024 * 1024 {
        return ERR_ARG;
    }
    let document = serde_json::json!({ "source": source, "highlights": relay_core::nix_editor::highlights(source) });
    give(json_out, &document.to_string())
}

#[no_mangle]
/// # Safety
/// `disk_path` must be null or a valid NUL-terminated path. `json_out` must
/// be a valid pointer to a string slot.
pub unsafe extern "C" fn relay_nixos_generations(
    disk_path: *const c_char,
    json_out: *mut *mut c_char,
) -> c_int {
    let path = match cstr(disk_path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if path.len() > 4096 || path.contains('\0') {
        return ERR_ARG;
    }
    match vm_generations_json(std::path::Path::new(path)) {
        Ok(json) => give(json_out, &json),
        Err(e) => {
            let _ = give(json_out, &e.to_string());
            map_err(e)
        }
    }
}

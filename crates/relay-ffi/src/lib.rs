//! C ABI. Keep `wawona_wasm_*` / `wwn_vm_*` in their own archives for overlap.

use relay_core::{RelayKind, RelaySpec, resolve_backend};
use relay_oci::prepare_bundle;
use relay_vm::{start as vm_start, stop as vm_stop, RelayHandle};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::Mutex;

static HANDLES: Mutex<Vec<RelayHandle>> = Mutex::new(Vec::new());

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

fn parse_spec(json: *const c_char) -> Result<RelaySpec, c_int> {
    let s = cstr(json)?;
    serde_json::from_str(s).map_err(|_| ERR_ARG)
}

#[no_mangle]
pub extern "C" fn relay_resolve_backend(
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
pub extern "C" fn relay_start(spec_json: *const c_char, handle_out: *mut *mut c_char) -> c_int {
    let spec = match parse_spec(spec_json) {
        Ok(s) => s,
        Err(c) => return c,
    };
    if spec.kind == RelayKind::Wasm {
        match relay_wasm::resolve(&spec) {
            Ok(b) => return give(handle_out, b.as_str()),
            Err(e) => {
                let _ = give(handle_out, &e.to_string());
                return map_err(e);
            }
        }
    }
    if let Err(e) = prepare_bundle(&spec) {
        let _ = give(handle_out, &e.to_string());
        return map_err(e);
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
pub extern "C" fn relay_stop(handle: *const c_char) -> c_int {
    let id = match cstr(handle) {
        Ok(s) => s,
        Err(c) => return c,
    };
    if let Ok(mut g) = HANDLES.lock() {
        g.retain(|h| h.id != id);
    }
    match vm_stop(id) {
        Ok(()) => OK,
        Err(e) => map_err(e),
    }
}

#[no_mangle]
pub extern "C" fn relay_wayland_endpoint(
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
pub extern "C" fn relay_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    }
}

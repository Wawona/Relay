//! C ABI for `wawona-dispatch.c`.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;

use crate::sandbox;

fn c_args(argc: c_int, argv: *const *const c_char) -> Vec<String> {
    let mut args = Vec::new();
    if argv.is_null() {
        return args;
    }
    for i in 0..argc.max(0) as isize {
        let p = unsafe { *argv.offset(i) };
        if p.is_null() {
            continue;
        }
        args.push(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned());
    }
    args
}

fn wasm_path_from_args(args: &[String]) -> Option<PathBuf> {
    if args.is_empty() {
        return None;
    }
    let first = args[0].as_str();
    if first == "wasm" || first.ends_with("/wasm") {
        return args.get(1).map(PathBuf::from);
    }
    Some(PathBuf::from(first))
}

/// `int wawona_wasm_can_run(const char *path)`
#[no_mangle]
pub unsafe extern "C" fn wawona_wasm_can_run(path: *const c_char) -> c_int {
    if path.is_null() {
        return 0;
    }
    let p = CStr::from_ptr(path).to_string_lossy();
    if sandbox::is_wasm_magic(std::path::Path::new(p.as_ref())) {
        1
    } else {
        0
    }
}

/// `int wawona_wasm_run(int argc, char **argv)`
#[no_mangle]
pub unsafe extern "C" fn wawona_wasm_run(argc: c_int, argv: *const *const c_char) -> c_int {
    let args = c_args(argc, argv);
    match std::panic::catch_unwind(|| crate::run_args(&args)) {
        Ok(Ok(code)) => code,
        Ok(Err(e)) => {
            // On Apple-mobile fake-TTY shells, host STDERR is the app log.
            // Print runtime errors on stdout so they reach weston-terminal.
            if std::env::var_os("WAWONA_PTY_FAKE_TTY").is_some() {
                println!("wawona-wasm: {e:#}");
            } else {
                eprintln!("wawona-wasm: {e:#}");
            }
            127
        }
        Err(_) => {
            if std::env::var_os("WAWONA_PTY_FAKE_TTY").is_some() {
                println!("wawona-wasm: panic in interpreter");
            } else {
                eprintln!("wawona-wasm: panic in interpreter");
            }
            127
        }
    }
}

/// `int wawona_terminal_raw_enabled(void)`
#[no_mangle]
pub extern "C" fn wawona_terminal_raw_enabled() -> c_int {
    crate::host::terminal_raw_enabled()
}

/// Keep `wpm_main` in `libwawona_wasm.a`.
///
/// Wawona xcodegen passes `-Wl,-u,_wpm_main` with lazy `-lwawona_wasm` so
/// `wawona-dispatch` can call in-process `wpm`. Without a root-crate reference,
/// rustc GC drops the symbol from the staticlib and macOS/iOS link fails with
/// `Undefined symbol: _wpm_main`.
#[used]
static KEEP_WPM_MAIN: unsafe extern "C" fn(c_int, *const *const c_char) -> c_int = wpm::ffi::wpm_main;

pub fn path_from_c_args(args: &[String]) -> Option<PathBuf> {
    wasm_path_from_args(args)
}

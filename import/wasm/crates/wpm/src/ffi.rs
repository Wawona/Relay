//! C ABI: `wpm_main` for wawona-dispatch (Apple mobile in-process).

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

use crate::cli_run;

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

/// `int wpm_main(int argc, char **argv)` — same shape as `phoon_main`.
#[no_mangle]
pub unsafe extern "C" fn wpm_main(argc: c_int, argv: *const *const c_char) -> c_int {
    let args = c_args(argc, argv);
    match std::panic::catch_unwind(|| cli_run(&args)) {
        Ok(Ok(code)) => code,
        Ok(Err(e)) => {
            eprintln!("wpm: {e:#}");
            1
        }
        Err(_) => {
            eprintln!("wpm: panic");
            127
        }
    }
}

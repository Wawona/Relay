//! Cooperative Ctrl+C for in-process WASI guests.
//!
//! Apple-mobile zsh runs `wawona_wasm_run` on the shell pthread. A raw
//! `pthread_kill(SIGINT)` is unsafe there (zsh handler / process abort).
//! Epoch trap plus shutting down the guest Wayland sockets stops the
//! interpreter and drops GUI toplevels (`client_disconnected`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use wasmtime::{Engine, Store};

static INTERRUPT: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static CURRENT_ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

pub fn is_set() -> bool {
    INTERRUPT.load(Ordering::SeqCst)
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

/// Wake blocking WASI stdin after Ctrl+C. Epoch alone does not run until
/// the host call returns; SIGUSR1 (empty handler, no SA_RESTART) interrupts
/// libc read so the interposed path can return EIO and the epoch trap fires.
pub fn request() {
    INTERRUPT.store(true, Ordering::SeqCst);
    if let Ok(guard) = CURRENT_ENGINE.lock() {
        if let Some(engine) = guard.as_ref() {
            engine.increment_epoch();
            // Extra ticks so a guest that already passed deadline 1 still traps.
            engine.increment_epoch();
        }
    }
    crate::host::shutdown_guest_sockets();
}

fn register_engine(engine: Engine) {
    INTERRUPT.store(false, Ordering::SeqCst);
    RUNNING.store(true, Ordering::SeqCst);
    if let Ok(mut guard) = CURRENT_ENGINE.lock() {
        *guard = Some(engine);
    }
}

fn unregister_engine() {
    RUNNING.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = CURRENT_ENGINE.lock() {
        *guard = None;
    }
}

/// Registers the live engine and arms an epoch trap. Drop shuts sockets
/// so the compositor closes the guest window even if the trap already fired.
pub struct RunSession {
    _private: (),
}

impl RunSession {
    pub fn begin<T>(engine: &Engine, store: &mut Store<T>) -> Self {
        store.set_epoch_deadline(1);
        store.epoch_deadline_trap();
        register_engine(engine.clone());
        Self { _private: () }
    }
}

impl Drop for RunSession {
    fn drop(&mut self) {
        crate::host::shutdown_guest_sockets();
        unregister_engine();
    }
}

/// POSIX-style 130 (128 + SIGINT) when Ctrl+C cancelled the guest.
pub fn map_run_result(result: anyhow::Result<i32>) -> anyhow::Result<i32> {
    if is_set() {
        return Ok(130);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_sets_flag_without_engine() {
        INTERRUPT.store(false, Ordering::SeqCst);
        RUNNING.store(false, Ordering::SeqCst);
        request();
        assert!(is_set());
        INTERRUPT.store(false, Ordering::SeqCst);
    }
}

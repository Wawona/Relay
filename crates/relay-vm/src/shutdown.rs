//! Observe owned threads until a deadline; never detach a live worker.

use relay_core::RelayError;
use std::{
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) fn join_until(
    thread: &mut Option<JoinHandle<()>>,
    deadline: Instant,
    name: &str,
) -> Result<bool, RelayError> {
    while thread.as_ref().is_some_and(|worker| !worker.is_finished()) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        thread::sleep(remaining.min(Duration::from_millis(2)));
    }
    if let Some(worker) = thread.take() {
        worker
            .join()
            .map_err(|_| RelayError::Failed(format!("{name} thread panicked")))?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[cfg(not(miri))]
    #[test]
    fn pending_cpu_retains_owned_peer_until_retry() {
        use std::{io::Read, os::unix::net::UnixStream};
        let (release, wait) = mpsc::channel();
        let (peer, mut native) = UnixStream::pair().unwrap();
        let mut cpu = Some(thread::spawn(move || {
            wait.recv().unwrap();
            drop(peer);
        }));
        let (native_done, completion) = mpsc::channel();
        let mut client = Some(thread::spawn(move || {
            assert_eq!(native.read(&mut [0]).unwrap(), 0);
            native_done.send(()).unwrap();
        }));
        let deadline = Instant::now();
        assert!(!join_until(&mut cpu, deadline, "test CPU").unwrap());
        assert!(cpu.is_some());
        assert!(!join_until(&mut client, deadline, "test native").unwrap());
        assert!(completion.try_recv().is_err());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        assert!(join_until(&mut cpu, deadline, "test CPU").unwrap());
        assert!(join_until(&mut client, deadline, "test native").unwrap());
        completion.recv().unwrap();
        assert!(cpu.is_none() && client.is_none());
    }

    #[test]
    fn panic_is_reaped_and_reported_without_detaching() {
        let mut worker = Some(thread::spawn(|| panic!("shutdown fixture")));
        let result = join_until(
            &mut worker,
            Instant::now() + Duration::from_secs(2),
            "test CPU",
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("test CPU thread panicked"));
        assert!(worker.is_none());
        assert!(join_until(&mut worker, Instant::now(), "test CPU").unwrap());
    }

    #[test]
    fn pending_join_keeps_worker_until_explicit_release() {
        let (release, wait) = mpsc::channel();
        let mut worker = Some(thread::spawn(move || wait.recv().unwrap()));
        assert!(!join_until(&mut worker, Instant::now(), "test worker").unwrap());
        assert!(worker.is_some());
        release.send(()).unwrap();
        assert!(join_until(
            &mut worker,
            Instant::now() + Duration::from_secs(2),
            "test worker"
        )
        .unwrap());
        assert!(worker.is_none());
    }
}

//! Native waypipe consumes the actual guest stream on a session-owned worker.
//! Transport activity and native exit codes are not authentication/frame proof.

use crate::VsockConnection;
use relay_core::RelayError;
use std::{
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Synchronous native entry. The descriptor is borrowed, never transferred.
/// Implementations must return when the guest channel closes and must not close
/// the borrowed descriptor. Any retained descriptor must be an owned duplicate.
pub type HostWaypipeEntry = unsafe extern "C" fn(i32) -> i32;

pub(crate) struct Worker {
    thread: Option<JoinHandle<()>>,
    last_exit: Arc<AtomicI32>,
}

impl Worker {
    /// # Safety
    /// Entry must obey HostWaypipeEntry's descriptor/lifetime contract and remain
    /// callable until the worker is joined. Native entry points are static.
    pub(crate) unsafe fn start(
        connections: Receiver<VsockConnection>,
        stop: Arc<AtomicBool>,
        vm_state: Arc<AtomicU8>,
        entry: HostWaypipeEntry,
    ) -> Result<Self, RelayError> {
        let last_exit = Arc::new(AtomicI32::new(i32::MIN));
        let exit = Arc::clone(&last_exit);
        let thread = thread::Builder::new()
            .name("relay-host-waypipe".into())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) && vm_state.load(Ordering::Acquire) == 0 {
                    match connections.recv_timeout(Duration::from_millis(20)) {
                        Ok(connection) => {
                            if stop.load(Ordering::Acquire) {
                                break;
                            }
                            // SAFETY: the worker owns connection.stream for the entire
                            // synchronous call; the caller supplied a contracted entry.
                            let result = unsafe { entry(connection.stream.as_raw_fd()) };
                            exit.store(result, Ordering::Release);
                            // Drop the real endpoint before accepting a reconnect.
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|error| RelayError::Failed(format!("cannot start host waypipe: {error}")))?;
        Ok(Self {
            thread: Some(thread),
            last_exit,
        })
    }

    pub(crate) fn last_exit(&self) -> Option<i32> {
        match self.last_exit.load(Ordering::Acquire) {
            i32::MIN => None,
            code => Some(code),
        }
    }

    /// A delayed foreign entry stays owned so Stop can be retried safely.
    #[cfg(test)]
    pub(crate) fn join_timeout(&mut self, timeout: Duration) -> Result<bool, RelayError> {
        self.join_until(Instant::now() + timeout)
    }

    pub(crate) fn join_until(&mut self, deadline: Instant) -> Result<bool, RelayError> {
        crate::shutdown::join_until(&mut self.thread, deadline, "host waypipe")
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::{
            fd::{BorrowedFd, OwnedFd},
            unix::net::UnixStream,
        },
        sync::{atomic::AtomicUsize, mpsc},
    };

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "C" fn echo(fd: i32) -> i32 {
        // SAFETY: the worker keeps its socket alive; clone preserves ownership.
        let fd: OwnedFd = unsafe { BorrowedFd::borrow_raw(fd) }
            .try_clone_to_owned()
            .unwrap();
        let mut stream = UnixStream::from(fd);
        let mut request = [0; 4];
        if stream.read_exact(&mut request).is_err() {
            return 1;
        }
        stream.write_all(&request).unwrap();
        CALLS.fetch_add(1, Ordering::Release);
        0
    }

    fn connection(stream: UnixStream) -> VsockConnection {
        VsockConnection {
            guest_port: 20,
            host_port: crate::WAYPIPE_VSOCK_PORT,
            stream,
        }
    }

    #[test]
    fn actual_stream_callback_reconnect_and_borrowed_ownership() {
        let (send, receive) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(AtomicU8::new(0));
        // SAFETY: fixed echo entry duplicates its borrowed socket and returns.
        let mut worker = unsafe { Worker::start(receive, Arc::clone(&stop), state, echo) }.unwrap();
        for bytes in [*b"boot", *b"next"] {
            let (mut peer, stream) = UnixStream::pair().unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            send.send(connection(stream)).unwrap();
            peer.write_all(&bytes).unwrap();
            let mut actual = [0; 4];
            peer.read_exact(&mut actual).unwrap();
            assert_eq!(actual, bytes);
            // Both the duplicate and worker-owned descriptor close after return.
            assert_eq!(peer.read(&mut [0]).unwrap(), 0);
        }
        stop.store(true, Ordering::Release);
        assert!(worker.join_timeout(Duration::from_secs(1)).unwrap());
        assert_eq!(worker.last_exit(), Some(0));
        assert_eq!(CALLS.load(Ordering::Acquire), 2);
    }

    #[test]
    fn stop_before_guest_never_calls_native_entry() {
        let (_send, receive) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(true));
        let state = Arc::new(AtomicU8::new(0));
        // SAFETY: no channel is available; echo also obeys the entry contract.
        let mut worker = unsafe { Worker::start(receive, stop, state, echo) }.unwrap();
        assert!(worker.join_timeout(Duration::from_secs(1)).unwrap());
        assert_eq!(worker.last_exit(), None);
    }

    #[test]
    fn device_peer_close_releases_active_native_read() {
        let (send, receive) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(AtomicU8::new(0));
        // SAFETY: echo's read returns when the device-owned peer closes.
        let mut worker = unsafe { Worker::start(receive, Arc::clone(&stop), state, echo) }.unwrap();
        let (mut peer, stream) = UnixStream::pair().unwrap();
        send.send(connection(stream)).unwrap();
        peer.write_all(b"x").unwrap();
        // A partial real header guarantees echo cannot finish with success.
        drop(peer);
        drop(send);
        assert!(worker.join_timeout(Duration::from_secs(1)).unwrap());
        assert_eq!(worker.last_exit(), Some(1));
        stop.store(true, Ordering::Release);
    }

    static ENTERED: AtomicBool = AtomicBool::new(false);
    static RELEASE: AtomicBool = AtomicBool::new(false);
    unsafe extern "C" fn delayed(_fd: i32) -> i32 {
        ENTERED.store(true, Ordering::Release);
        while !RELEASE.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(1));
        }
        0
    }

    #[test]
    fn delayed_foreign_entry_remains_owned_until_retry() {
        let (send, receive) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(AtomicU8::new(0));
        // SAFETY: this fixed delayed entry never accesses the borrowed descriptor.
        let mut worker =
            unsafe { Worker::start(receive, Arc::clone(&stop), state, delayed) }.unwrap();
        let (_peer, stream) = UnixStream::pair().unwrap();
        send.send(connection(stream)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !ENTERED.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(ENTERED.load(Ordering::Acquire));
        stop.store(true, Ordering::Release);
        assert!(!worker.join_timeout(Duration::from_millis(10)).unwrap());
        assert!(worker.thread.is_some());
        RELEASE.store(true, Ordering::Release);
        assert!(worker.join_timeout(Duration::from_secs(1)).unwrap());
        assert!(worker.thread.is_none());
    }
}

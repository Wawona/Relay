//! Host side of the guest session control channel (vsock port 1025).
//! Listens on a Unix stream that the virtio-vsock stack forwards from the
//! guest's AF_VSOCK connect to host CID 2 port 1025.

use crate::guest_session::{
    verify_response, SessionChallenge, SessionKey, SessionResponse, CONTROL_VSOCK_PORT,
};
use relay_core::RelayError;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Outcome of one authenticated readiness exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedReady {
    pub machine_id: String,
    pub session_id: String,
    pub unit_state: String,
}

pub struct AuthListener {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    ready: Arc<Mutex<Option<AuthenticatedReady>>>,
}

impl AuthListener {
    /// Serve the control protocol on a virtio-vsock host stream.
    pub fn serve_connection(
        connections: std::sync::mpsc::Receiver<crate::vsock::VsockConnection>,
        key: SessionKey,
        challenge: SessionChallenge,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let ready = Arc::new(Mutex::new(None));
        let ready_w = Arc::clone(&ready);
        let stop_w = Arc::clone(&stop);
        let join = thread::spawn(move || {
            while !stop_w.load(Ordering::Acquire) {
                match connections.recv_timeout(Duration::from_millis(50)) {
                    Ok(connection) => {
                        if let Ok(outcome) = serve_one(connection.stream, &key, &challenge) {
                            if let Ok(mut guard) = ready_w.lock() {
                                *guard = Some(outcome);
                            }
                            return;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        });
        Self {
            stop,
            join: Some(join),
            ready,
        }
    }

    pub fn start(
        socket_path: &Path,
        key: SessionKey,
        challenge: SessionChallenge,
    ) -> Result<Self, RelayError> {
        if socket_path.exists() {
            let _ = std::fs::remove_file(socket_path);
        }
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RelayError::Failed(format!("session auth socket dir: {e}"))
            })?;
        }
        let listener = UnixListener::bind(socket_path).map_err(|e| {
            RelayError::Failed(format!("session auth listen: {e}"))
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|e| RelayError::Failed(format!("session auth nonblocking: {e}")))?;
        let stop = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(Mutex::new(None));
        let stop_w = Arc::clone(&stop);
        let ready_w = Arc::clone(&ready);
        let join = thread::spawn(move || {
            run_listener(listener, key, challenge, stop_w, ready_w);
        });
        Ok(Self {
            stop,
            join: Some(join),
            ready,
        })
    }

    pub fn control_port() -> u32 {
        CONTROL_VSOCK_PORT
    }

    pub fn take_ready(&self) -> Option<AuthenticatedReady> {
        self.ready.lock().ok().and_then(|mut g| g.take())
    }

    pub fn peek_ready(&self) -> Option<AuthenticatedReady> {
        self.ready.lock().ok().and_then(|g| g.clone())
    }
}

impl Drop for AuthListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_listener(
    listener: UnixListener,
    key: SessionKey,
    challenge: SessionChallenge,
    stop: Arc<AtomicBool>,
    ready: Arc<Mutex<Option<AuthenticatedReady>>>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Ok(outcome) = serve_one(stream, &key, &challenge) {
                    if let Ok(mut guard) = ready.lock() {
                        *guard = Some(outcome);
                    }
                    return;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn serve_one(
    stream: UnixStream,
    key: &SessionKey,
    challenge: &SessionChallenge,
) -> Result<AuthenticatedReady, RelayError> {
    stream
        .set_read_timeout(Some(Duration::from_secs(45)))
        .map_err(|e| RelayError::Failed(format!("auth timeout: {e}")))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(45)))
        .map_err(|e| RelayError::Failed(format!("auth timeout: {e}")))?;
    let mut writer = stream.try_clone().map_err(|e| {
        RelayError::Failed(format!("auth clone: {e}"))
    })?;
    let mut reader = BufReader::new(stream);
    let key_line = format!("WWN1 KEY {}\n", hex(key.as_bytes()));
    writer
        .write_all(key_line.as_bytes())
        .map_err(|e| RelayError::Failed(format!("auth key write: {e}")))?;
    writer
        .write_all(challenge.encode().as_bytes())
        .map_err(|e| RelayError::Failed(format!("auth challenge write: {e}")))?;
    writer
        .flush()
        .map_err(|e| RelayError::Failed(format!("auth flush: {e}")))?;
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| RelayError::Failed(format!("auth read: {e}")))?;
    let response = SessionResponse::parse(&line)?;
    verify_response(challenge, &response, key)?;
    writer
        .write_all(b"WWN1 OK\n")
        .map_err(|e| RelayError::Failed(format!("auth ack: {e}")))?;
    Ok(AuthenticatedReady {
        machine_id: response.machine_id,
        session_id: response.session_id,
        unit_state: response.unit_state,
    })
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_session::SessionResponse;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    #[test]
    fn host_accepts_valid_guest_response() {
        let key = SessionKey::derive(&[4u8; 32], "khash", "rhash");
        let challenge =
            SessionChallenge::new("E4A1C0DE", "sess", "khash", "rhash", [5u8; 32]);
        let (host, mut guest) = UnixStream::pair().unwrap();
        guest.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        guest.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        tx.send(crate::vsock::VsockConnection {
            guest_port: 9,
            host_port: CONTROL_VSOCK_PORT,
            stream: host,
        })
        .unwrap();
        drop(tx);
        let stop = Arc::new(AtomicBool::new(false));
        let listener = AuthListener::serve_connection(rx, key.clone(), challenge.clone(), stop);
        let mut reader = BufReader::new(guest.try_clone().unwrap());
        let mut key_line = String::new();
        let mut chal_line = String::new();
        reader.read_line(&mut key_line).unwrap();
        reader.read_line(&mut chal_line).unwrap();
        assert!(key_line.starts_with("WWN1 KEY "));
        let response = SessionResponse::sign(&challenge, "multi-user+wawona-session", &key);
        guest.write_all(response.encode().as_bytes()).unwrap();
        let mut ack = String::new();
        reader.read_line(&mut ack).unwrap();
        assert!(ack.starts_with("WWN1 OK"));
        for _ in 0..50 {
            if listener.peek_ready().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let ready = listener.take_ready().expect("authenticated ready");
        assert_eq!(ready.machine_id, "E4A1C0DE");
        assert_eq!(ready.unit_state, "multi-user+wawona-session");
    }
}

//! Bounded byte forwarding between already-connected transport sockets.
//! This does not authenticate a guest or declare graphics readiness.

use std::{
    io::{self, Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
};

const CAPACITY: usize = 64 * 1024;

/// Result of one bounded, nonblocking forwarding turn.
#[derive(Debug, PartialEq, Eq)]
pub enum BridgeProgress {
    Idle,
    Progress,
    Closed,
}

/// Owns both endpoints. Call `advance` from the session's transport loop.
/// EOF drains that direction before forwarding FIN; the reverse stays live.
pub struct StreamBridge {
    streams: Option<(UnixStream, UnixStream)>,
    forward: Direction,
    reverse: Direction,
}

impl StreamBridge {
    pub fn new(left: UnixStream, right: UnixStream) -> io::Result<Self> {
        left.set_nonblocking(true)?;
        right.set_nonblocking(true)?;
        Ok(Self {
            streams: Some((left, right)),
            forward: Direction::new(),
            reverse: Direction::new(),
        })
    }

    /// Each direction attempts at most one read, two writes and one shutdown.
    /// Errors close both owned endpoints. No blocking connect or worker is hidden.
    pub fn advance(&mut self) -> io::Result<BridgeProgress> {
        let Some((left, right)) = &self.streams else {
            return Ok(BridgeProgress::Closed);
        };
        let result = (|| {
            let forward = self.forward.advance(left, right)?;
            let reverse = self.reverse.advance(right, left)?;
            Ok(forward || reverse)
        })();
        match result {
            Err(error) => {
                self.cancel();
                Err(error)
            }
            Ok(_) if self.forward.finished && self.reverse.finished => {
                self.cancel();
                Ok(BridgeProgress::Closed)
            }
            Ok(true) => Ok(BridgeProgress::Progress),
            Ok(false) => Ok(BridgeProgress::Idle),
        }
    }

    /// Immediate session cancellation discards pending bytes and closes peers.
    pub fn cancel(&mut self) {
        if let Some((left, right)) = self.streams.take() {
            let _ = left.shutdown(Shutdown::Both);
            let _ = right.shutdown(Shutdown::Both);
        }
        self.forward.start = 0;
        self.forward.end = 0;
        self.reverse.start = 0;
        self.reverse.end = 0;
    }
}

impl Drop for StreamBridge {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct Direction {
    bytes: Box<[u8]>,
    start: usize,
    end: usize,
    eof: bool,
    finished: bool,
}

/// Validate byte consumption before changing the pending range.
fn consume(start: usize, end: usize, count: usize) -> Option<(usize, usize)> {
    if start > end || end > CAPACITY || count > end - start {
        return None;
    }
    let next = start + count;
    Some(if next == end { (0, 0) } else { (next, end) })
}

impl Direction {
    fn new() -> Self {
        Self {
            bytes: vec![0; CAPACITY].into_boxed_slice(),
            start: 0,
            end: 0,
            eof: false,
            finished: false,
        }
    }

    fn flush(&mut self, mut output: &UnixStream) -> io::Result<bool> {
        if self.start == self.end {
            return Ok(false);
        }
        match output.write(&self.bytes[self.start..self.end]) {
            Ok(0) => Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => {
                (self.start, self.end) = consume(self.start, self.end, count)
                    .ok_or_else(|| io::Error::other("invalid stream write length"))?;
                Ok(true)
            }
            Err(error) if retry_later(&error) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn advance(&mut self, mut input: &UnixStream, output: &UnixStream) -> io::Result<bool> {
        if self.finished {
            return Ok(false);
        }
        let mut progress = self.flush(output)?;
        if !self.eof && self.start == self.end {
            match input.read(&mut self.bytes) {
                Ok(0) => {
                    self.eof = true;
                    progress = true;
                }
                Ok(count) => {
                    self.end = count;
                    progress = true;
                }
                Err(error) if retry_later(&error) => {}
                Err(error) => return Err(error),
            }
            progress |= self.flush(output)?;
        }
        if self.eof && self.start == self.end {
            match output.shutdown(Shutdown::Write) {
                Ok(()) => {
                    self.finished = true;
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        Ok(progress)
    }
}

fn retry_later(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

#[cfg(kani)]
#[kani::proof]
fn consumption_preserves_bounded_pending_bytes() {
    let start: usize = kani::any();
    let end: usize = kani::any();
    let count: usize = kani::any();
    if let Some((next_start, next_end)) = consume(start, end, count) {
        assert!(start <= end && end <= CAPACITY);
        assert!(count <= end - start);
        assert!(next_start <= next_end && next_end <= CAPACITY);
        assert_eq!(next_end - next_start, end - start - count);
    } else {
        assert!(start > end || end > CAPACITY || count > end - start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (UnixStream, StreamBridge, UnixStream) {
        let (left, bridge_left) = UnixStream::pair().unwrap();
        let (bridge_right, right) = UnixStream::pair().unwrap();
        left.set_nonblocking(true).unwrap();
        right.set_nonblocking(true).unwrap();
        (
            left,
            StreamBridge::new(bridge_left, bridge_right).unwrap(),
            right,
        )
    }

    fn collect(socket: &mut UnixStream, bytes: &mut Vec<u8>) -> bool {
        let mut chunk = [0; 8192];
        match socket.read(&mut chunk) {
            Ok(0) => true,
            Ok(count) => {
                bytes.extend_from_slice(&chunk[..count]);
                false
            }
            Err(error) if retry_later(&error) => false,
            Err(error) => panic!("peer read failed: {error}"),
        }
    }

    #[test]
    fn half_close_drains_bytes_and_keeps_reverse_live() {
        let (mut left, mut bridge, mut right) = setup();
        left.write_all(b"guest request").unwrap();
        left.shutdown(Shutdown::Write).unwrap();
        let mut request = Vec::new();
        let mut eof = false;
        for _ in 0..100 {
            assert_ne!(bridge.advance().unwrap(), BridgeProgress::Closed);
            eof |= collect(&mut right, &mut request);
            if eof {
                break;
            }
        }
        assert!(eof);
        assert_eq!(request, b"guest request");
        right.write_all(b"host response after FIN").unwrap();
        right.shutdown(Shutdown::Write).unwrap();
        let mut response = Vec::new();
        let mut closed = false;
        for _ in 0..100 {
            closed |= bridge.advance().unwrap() == BridgeProgress::Closed;
            collect(&mut left, &mut response);
            if closed {
                break;
            }
        }
        assert!(closed);
        assert_eq!(response, b"host response after FIN");
    }

    #[test]
    fn bidirectional_backpressure_preserves_order_and_bounds() {
        let (mut left, mut bridge, mut right) = setup();
        let guest: Vec<_> = (0..CAPACITY * 8).map(|i| (i * 17) as u8).collect();
        let host: Vec<_> = (0..CAPACITY * 9).map(|i| (i * 29) as u8).collect();
        let (mut guest_sent, mut host_sent) = (0, 0);
        let (mut at_host, mut at_guest) = (Vec::new(), Vec::new());
        for turn in 0..20_000 {
            for (socket, source, sent) in [
                (&mut left, &guest, &mut guest_sent),
                (&mut right, &host, &mut host_sent),
            ] {
                if *sent < source.len() {
                    match socket.write(&source[*sent..]) {
                        Ok(count) => *sent += count,
                        Err(error) if retry_later(&error) => {}
                        Err(error) => panic!("peer write failed: {error}"),
                    }
                }
            }
            bridge.advance().unwrap();
            assert!(bridge.forward.end <= CAPACITY && bridge.reverse.end <= CAPACITY);
            // Hold both readers to fill real OS buffers before releasing them.
            if turn >= 100 {
                collect(&mut right, &mut at_host);
                collect(&mut left, &mut at_guest);
            }
            if at_host.len() == guest.len() && at_guest.len() == host.len() {
                break;
            }
        }
        assert_eq!(at_host, guest);
        assert_eq!(at_guest, host);
    }

    #[test]
    fn cancellation_and_drop_close_owned_endpoints() {
        let (mut left, mut bridge, mut right) = setup();
        bridge.cancel();
        assert_eq!(bridge.advance().unwrap(), BridgeProgress::Closed);
        assert_eq!(left.read(&mut [0]).unwrap(), 0);
        assert_eq!(right.read(&mut [0]).unwrap(), 0);
        let (mut left, bridge, mut right) = setup();
        drop(bridge);
        assert_eq!(left.read(&mut [0]).unwrap(), 0);
        assert_eq!(right.read(&mut [0]).unwrap(), 0);
    }

    #[test]
    fn peer_error_cancels_both_directions() {
        let (mut left, mut bridge, right) = setup();
        // Darwin SHUT_RD alone may still accept peer writes. A disconnected
        // peer exercises the fatal write path on every supported Unix host.
        drop(right);
        left.write_all(b"pending").unwrap();
        assert!(bridge.advance().is_err());
        assert_eq!(bridge.advance().unwrap(), BridgeProgress::Closed);
        assert!(bridge.streams.is_none());
        assert_eq!(left.read(&mut [0]).unwrap(), 0);
    }

    #[test]
    fn pending_range_rejects_invalid_and_clears_drained() {
        assert_eq!(consume(0, 3, 2), Some((2, 3)));
        assert_eq!(consume(2, 3, 1), Some((0, 0)));
        assert_eq!(consume(1, 0, 0), None);
        assert_eq!(consume(0, CAPACITY + 1, 0), None);
        assert_eq!(consume(0, 3, 4), None);
        assert_eq!(consume(0, CAPACITY, usize::MAX), None);
    }
}

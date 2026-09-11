//! In-process bounded vsock channels. Waypipe binds only through this Relay boundary.
use relay_core::RelayError;
use std::collections::{BTreeMap, VecDeque};
pub(crate) struct Vsock {
    channels: BTreeMap<u32, VecDeque<Vec<u8>>>,
    limit: usize,
}
impl Vsock {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            channels: BTreeMap::new(),
            limit,
        }
    }
    pub(crate) fn open(&mut self, port: u32) {
        self.channels.entry(port).or_default();
    }
    pub(crate) fn send(&mut self, port: u32, bytes: Vec<u8>) -> Result<(), RelayError> {
        if bytes.len() > self.limit {
            return Err(RelayError::Failed(
                "vsock packet exceeds Relay limit".into(),
            ));
        }
        self.channels
            .get_mut(&port)
            .ok_or_else(|| RelayError::Failed("vsock port not open".into()))?
            .push_back(bytes);
        Ok(())
    }
    pub(crate) fn receive(&mut self, port: u32) -> Option<Vec<u8>> {
        self.channels.get_mut(&port)?.pop_front()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vsock_delivers_bounded_packets() {
        let mut v = Vsock::new(8);
        v.open(1024);
        v.send(1024, b"hello".to_vec()).unwrap();
        assert_eq!(v.receive(1024).unwrap(), b"hello");
        assert!(v.send(1024, vec![0; 9]).is_err());
    }
}

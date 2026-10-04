//! Virtio 1.2 stream packet ABI and modular credit arithmetic.
//! The wire codec is not authentication or guest readiness.
use relay_core::RelayError;

pub(crate) const HEADER_BYTES: usize = 44;
pub(crate) const MAX_DATA: usize = 64 * 1024;
pub(crate) const STREAM: u16 = 1;
pub(crate) const REQUEST: u16 = 1;
pub(crate) const RESPONSE: u16 = 2;
pub(crate) const RESET: u16 = 3;
pub(crate) const SHUTDOWN: u16 = 4;
pub(crate) const DATA: u16 = 5;
pub(crate) const CREDIT_UPDATE: u16 = 6;
pub(crate) const CREDIT_REQUEST: u16 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub src_cid: u64,
    pub dst_cid: u64,
    pub src_port: u32,
    pub dst_port: u32,
    pub len: u32,
    pub kind: u16,
    pub op: u16,
    pub flags: u32,
    pub buf_alloc: u32,
    pub fwd_cnt: u32,
}

impl Header {
    /// Unknown types/operations remain visible so the transport can send RST.
    /// Reject malformed packet lengths before allocating or accepting a flow.
    pub(crate) fn decode(packet: &[u8]) -> Result<(Self, &[u8]), RelayError> {
        if packet.len() < HEADER_BYTES || packet.len() > HEADER_BYTES + MAX_DATA {
            return Err(RelayError::Failed("vsock packet size invalid".into()));
        }
        let h = Self {
            src_cid: u64::from_le_bytes(packet[0..8].try_into().unwrap()),
            dst_cid: u64::from_le_bytes(packet[8..16].try_into().unwrap()),
            src_port: u32::from_le_bytes(packet[16..20].try_into().unwrap()),
            dst_port: u32::from_le_bytes(packet[20..24].try_into().unwrap()),
            len: u32::from_le_bytes(packet[24..28].try_into().unwrap()),
            kind: u16::from_le_bytes(packet[28..30].try_into().unwrap()),
            op: u16::from_le_bytes(packet[30..32].try_into().unwrap()),
            flags: u32::from_le_bytes(packet[32..36].try_into().unwrap()),
            buf_alloc: u32::from_le_bytes(packet[36..40].try_into().unwrap()),
            fwd_cnt: u32::from_le_bytes(packet[40..44].try_into().unwrap()),
        };
        if h.len as usize != packet.len() - HEADER_BYTES {
            return Err(RelayError::Failed("vsock payload length mismatch".into()));
        }
        Ok((h, &packet[HEADER_BYTES..]))
    }

    pub(crate) fn encode(self, payload: &[u8]) -> Result<Vec<u8>, RelayError> {
        if payload.len() > MAX_DATA || self.len as usize != payload.len() {
            return Err(RelayError::Failed("vsock payload length mismatch".into()));
        }
        let mut packet = Vec::with_capacity(HEADER_BYTES + payload.len());
        packet.extend_from_slice(&self.src_cid.to_le_bytes());
        packet.extend_from_slice(&self.dst_cid.to_le_bytes());
        packet.extend_from_slice(&self.src_port.to_le_bytes());
        packet.extend_from_slice(&self.dst_port.to_le_bytes());
        packet.extend_from_slice(&self.len.to_le_bytes());
        packet.extend_from_slice(&self.kind.to_le_bytes());
        packet.extend_from_slice(&self.op.to_le_bytes());
        packet.extend_from_slice(&self.flags.to_le_bytes());
        packet.extend_from_slice(&self.buf_alloc.to_le_bytes());
        packet.extend_from_slice(&self.fwd_cnt.to_le_bytes());
        packet.extend_from_slice(payload);
        Ok(packet)
    }
}

/// Counters wrap modulo 2^32. Subtract outstanding bytes before allowing a send;
/// Forward-counter advancement must be validated separately against sent bytes.
pub(crate) fn available_credit(allocated: u32, transmitted: u32, forwarded: u32) -> Option<u32> {
    allocated.checked_sub(transmitted.wrapping_sub(forwarded))
}

#[cfg(kani)]
#[kani::proof]
fn stream_credit_never_exceeds_peer_window() {
    let allocated: u32 = kani::any();
    let transmitted: u32 = kani::any();
    let forwarded: u32 = kani::any();
    let outstanding = transmitted.wrapping_sub(forwarded);
    match available_credit(allocated, transmitted, forwarded) {
        Some(free) => {
            assert!(free <= allocated);
            assert_eq!(free.checked_add(outstanding), Some(allocated));
        }
        None => assert!(outstanding > allocated),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_uapi_packet_fixture_and_boundary_rejections() {
        // Fixed little-endian virtio_vsock_hdr fixture, not codec-produced data.
        let raw = [
            3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0x78, 0x56, 0x34, 0x12, 0, 4, 0, 0, 3,
            0, 0, 0, 1, 0, 5, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0xff, 0xff, 0xff, 0xff, b'a', b'b', b'c',
        ];
        let (h, payload) = Header::decode(&raw).unwrap();
        assert_eq!(
            (h.src_cid, h.dst_cid, h.src_port, h.dst_port),
            (3, 2, 0x12345678, 1024)
        );
        assert_eq!(
            (h.kind, h.op, h.buf_alloc, h.fwd_cnt),
            (STREAM, DATA, 65536, u32::MAX)
        );
        assert_eq!(payload, b"abc");
        assert_eq!(h.encode(payload).unwrap(), raw);
        for n in 0..HEADER_BYTES {
            assert!(Header::decode(&raw[..n]).is_err());
        }
        assert!(Header::decode(&raw[..raw.len() - 1]).is_err());
        let mut oversized = raw.to_vec();
        oversized[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Header::decode(&oversized).is_err());
        assert!(h.encode(b"abcd").is_err());
    }

    #[test]
    fn credit_wrap_zero_window_and_invalid_forwarding() {
        assert_eq!(available_credit(65536, 3, u32::MAX - 4), Some(65528));
        assert_eq!(available_credit(0, 10, 10), Some(0));
        assert_eq!(available_credit(8, 10, 2), Some(0));
        assert_eq!(available_credit(8, 11, 2), None);
        assert_eq!(available_credit(65536, 10, 11), None);
    }
}

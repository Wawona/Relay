//! Session authentication for guest readiness.
//!
//! Host issues a fresh challenge bound to machine/session IDs and verified
//! guest artifact hashes. The guest agent answers with HMAC-SHA256 over that
//! binding plus reported unit state. A console READY string alone is never
//! acceptance. This is session authentication, not hardware attestation.

use relay_core::RelayError;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Control protocol version on the guest-agent vsock port.
pub const PROTOCOL_VERSION: u32 = 1;

/// Guest agent listens on this vsock port (host CID 2 connects after boot).
pub const CONTROL_VSOCK_PORT: u32 = 1025;

/// Host challenge for one StaticCpu / waypipe session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionChallenge {
    pub version: u32,
    pub machine_id: String,
    pub session_id: String,
    pub kernel_sha256: String,
    pub rootfs_sha256: String,
    pub nonce: [u8; 32],
}

/// Guest response after multi-user and the Wayland unit are up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResponse {
    pub version: u32,
    pub machine_id: String,
    pub session_id: String,
    pub kernel_sha256: String,
    pub rootfs_sha256: String,
    pub unit_state: String,
    pub mac: [u8; 32],
}

/// Ephemeral session key. Host generates it at Start and provisions it to the
/// guest agent over the control channel before the challenge. Never a bundled
/// static product secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionKey([u8; 32]);

impl SessionKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derive a key from a host-only seed and the verified artifact hashes.
    /// The seed must be fresh per Start (OS random). Artifact hashes bind the
    /// key to the image the host already validated.
    pub fn derive(seed: &[u8; 32], kernel_sha256: &str, rootfs_sha256: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"wawona-relay-session-v1\0");
        hasher.update(seed);
        hasher.update(kernel_sha256.as_bytes());
        hasher.update(b"\0");
        hasher.update(rootfs_sha256.as_bytes());
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Self(out)
    }
}

impl SessionChallenge {
    pub fn new(
        machine_id: impl Into<String>,
        session_id: impl Into<String>,
        kernel_sha256: impl Into<String>,
        rootfs_sha256: impl Into<String>,
        nonce: [u8; 32],
    ) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            machine_id: machine_id.into(),
            session_id: session_id.into(),
            kernel_sha256: kernel_sha256.into(),
            rootfs_sha256: rootfs_sha256.into(),
            nonce,
        }
    }

    fn binding_bytes(&self, unit_state: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(self.machine_id.as_bytes());
        out.push(0);
        out.extend_from_slice(self.session_id.as_bytes());
        out.push(0);
        out.extend_from_slice(self.kernel_sha256.as_bytes());
        out.push(0);
        out.extend_from_slice(self.rootfs_sha256.as_bytes());
        out.push(0);
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(unit_state.as_bytes());
        out
    }

    pub fn encode(&self) -> String {
        format!(
            "WWN1 CHALLENGE {} {} {} {} {} {}\n",
            self.version,
            self.machine_id,
            self.session_id,
            self.kernel_sha256,
            self.rootfs_sha256,
            hex(&self.nonce)
        )
    }
}

impl SessionResponse {
    pub fn sign(challenge: &SessionChallenge, unit_state: &str, key: &SessionKey) -> Self {
        let mac = hmac_mac(key, &challenge.binding_bytes(unit_state));
        Self {
            version: challenge.version,
            machine_id: challenge.machine_id.clone(),
            session_id: challenge.session_id.clone(),
            kernel_sha256: challenge.kernel_sha256.clone(),
            rootfs_sha256: challenge.rootfs_sha256.clone(),
            unit_state: unit_state.to_string(),
            mac,
        }
    }

    pub fn encode(&self) -> String {
        format!(
            "WWN1 READY {} {} {} {} {} {} {}\n",
            self.version,
            self.machine_id,
            self.session_id,
            self.kernel_sha256,
            self.rootfs_sha256,
            self.unit_state,
            hex(&self.mac)
        )
    }

    pub fn parse(line: &str) -> Result<Self, RelayError> {
        let line = line.trim();
        let rest = line
            .strip_prefix("WWN1 READY ")
            .ok_or_else(|| RelayError::Failed("guest session response missing WWN1 READY".into()))?;
        let mut parts = rest.splitn(7, ' ');
        let version = parts
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| RelayError::Failed("guest session response missing version".into()))?;
        let machine_id = parts
            .next()
            .ok_or_else(|| RelayError::Failed("guest session response missing machine_id".into()))?
            .to_string();
        let session_id = parts
            .next()
            .ok_or_else(|| RelayError::Failed("guest session response missing session_id".into()))?
            .to_string();
        let kernel_sha256 = parts
            .next()
            .ok_or_else(|| RelayError::Failed("guest session response missing kernel hash".into()))?
            .to_string();
        let rootfs_sha256 = parts
            .next()
            .ok_or_else(|| RelayError::Failed("guest session response missing rootfs hash".into()))?
            .to_string();
        let unit_state = parts
            .next()
            .ok_or_else(|| RelayError::Failed("guest session response missing unit_state".into()))?
            .to_string();
        let mac_hex = parts
            .next()
            .ok_or_else(|| RelayError::Failed("guest session response missing mac".into()))?;
        let mac = parse_hex32(mac_hex)?;
        Ok(Self {
            version,
            machine_id,
            session_id,
            kernel_sha256,
            rootfs_sha256,
            unit_state,
            mac,
        })
    }
}

/// Verify a guest READY response against the host challenge and session key.
pub fn verify_response(
    challenge: &SessionChallenge,
    response: &SessionResponse,
    key: &SessionKey,
) -> Result<(), RelayError> {
    if response.version != PROTOCOL_VERSION || challenge.version != PROTOCOL_VERSION {
        return Err(RelayError::Failed(
            "guest session protocol version mismatch".into(),
        ));
    }
    if response.machine_id != challenge.machine_id
        || response.session_id != challenge.session_id
        || response.kernel_sha256 != challenge.kernel_sha256
        || response.rootfs_sha256 != challenge.rootfs_sha256
    {
        return Err(RelayError::Failed(
            "guest session response does not match challenge binding".into(),
        ));
    }
    if response.unit_state != "multi-user+wawona-session" {
        return Err(RelayError::Failed(format!(
            "guest unit state not accepted: {}",
            response.unit_state
        )));
    }
    let expected = hmac_mac(key, &challenge.binding_bytes(&response.unit_state));
    if !constant_time_eq(&expected, &response.mac) {
        return Err(RelayError::Failed(
            "guest session MAC verification failed".into(),
        ));
    }
    Ok(())
}

/// SHA-256 of a presented SHM frame. Empty buffers are rejected.
pub fn frame_hash(pixels: &[u8]) -> Result<[u8; 32], RelayError> {
    if pixels.is_empty() {
        return Err(RelayError::Failed(
            "imported guest frame must be non-empty".into(),
        ));
    }
    let digest = Sha256::digest(pixels);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok(out)
}

fn hmac_mac(key: &SessionKey, data: &[u8]) -> [u8; 32] {
    // HMAC-SHA256 with a 32-byte key (RFC 2104). sha2 is already a crate dep.
    const BLOCK: usize = 64;
    let mut k_ipad = [0x36u8; BLOCK];
    let mut k_opad = [0x5cu8; BLOCK];
    for (i, b) in key.as_bytes().iter().enumerate() {
        k_ipad[i] ^= b;
        k_opad[i] ^= b;
    }
    let mut inner = Sha256::new();
    inner.update(k_ipad);
    inner.update(data);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(k_opad);
    outer.update(inner_digest);
    let digest = outer.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

fn parse_hex32(text: &str) -> Result<[u8; 32], RelayError> {
    if text.len() != 64 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(RelayError::Failed(
            "guest session MAC must be 32-byte hex".into(),
        ));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in text.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16)
            .map_err(|_| RelayError::Failed("guest session MAC hex parse failed".into()))?;
    }
    Ok(out)
}

fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_response_round_trip_authenticates() {
        let key = SessionKey::derive(
            &[7u8; 32],
            "aa".repeat(32).as_str(),
            "bb".repeat(32).as_str(),
        );
        let challenge = SessionChallenge::new(
            "E4A1C0DE",
            "session-1",
            "aa".repeat(32),
            "bb".repeat(32),
            [9u8; 32],
        );
        let response = SessionResponse::sign(&challenge, "multi-user+wawona-session", &key);
        verify_response(&challenge, &response, &key).unwrap();
        let parsed = SessionResponse::parse(&response.encode()).unwrap();
        verify_response(&challenge, &parsed, &key).unwrap();
    }

    #[test]
    fn wrong_key_or_unit_state_is_rejected() {
        let key = SessionKey::derive(&[1u8; 32], "k", "r");
        let other = SessionKey::derive(&[2u8; 32], "k", "r");
        let challenge = SessionChallenge::new("m", "s", "k", "r", [3u8; 32]);
        let good = SessionResponse::sign(&challenge, "multi-user+wawona-session", &key);
        assert!(verify_response(&challenge, &good, &other).is_err());
        let bad_unit = SessionResponse::sign(&challenge, "basic", &key);
        assert!(verify_response(&challenge, &bad_unit, &key).is_err());
    }

    #[test]
    fn empty_frame_is_rejected_and_nonempty_hashes() {
        assert!(frame_hash(&[]).is_err());
        let a = frame_hash(&[1, 2, 3, 4]).unwrap();
        let b = frame_hash(&[1, 2, 3, 5]).unwrap();
        assert_ne!(a, b);
    }
}

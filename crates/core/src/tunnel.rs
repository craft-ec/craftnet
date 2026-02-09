//! TCP tunnel types for SOCKS5 proxy mode
//!
//! When the first byte of a reconstructed payload is `PAYLOAD_MODE_TUNNEL`,
//! the exit node switches from HTTP mode to TCP tunnel mode. The exit opens
//! a raw TCP connection and pipes bytes bidirectionally.

use serde::{Deserialize, Serialize};

use crate::Id;

/// Payload prefix: HTTP mode (existing behavior)
pub const PAYLOAD_MODE_HTTP: u8 = 0x00;

/// Payload prefix: TCP tunnel mode (SOCKS5 proxy)
pub const PAYLOAD_MODE_TUNNEL: u8 = 0x01;

/// Metadata for a TCP tunnel session.
///
/// Serialized into the first chunk's payload after the mode byte.
/// All bursts within the same SOCKS5 CONNECT share the same `session_id`
/// so the exit can map them to the same TCP connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelMetadata {
    /// Destination hostname (e.g., "youtube.com")
    pub host: String,
    /// Destination port (e.g., 443)
    pub port: u16,
    /// Session ID shared across all bursts for one SOCKS5 connection
    pub session_id: Id,
    /// Signals session teardown (exit should close the TCP connection)
    pub is_close: bool,
}

impl TunnelMetadata {
    /// Serialize to bytes using bincode
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("TunnelMetadata serialization should not fail")
    }

    /// Deserialize from bytes using bincode
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tunnel_metadata_roundtrip() {
        let meta = TunnelMetadata {
            host: "example.com".to_string(),
            port: 443,
            session_id: [42u8; 32],
            is_close: false,
        };

        let bytes = meta.to_bytes();
        let decoded = TunnelMetadata::from_bytes(&bytes).unwrap();
        assert_eq!(meta, decoded);
    }

    #[test]
    fn test_tunnel_metadata_close_signal() {
        let meta = TunnelMetadata {
            host: String::new(),
            port: 0,
            session_id: [1u8; 32],
            is_close: true,
        };

        let bytes = meta.to_bytes();
        let decoded = TunnelMetadata::from_bytes(&bytes).unwrap();
        assert!(decoded.is_close);
    }

    #[test]
    fn test_payload_mode_constants() {
        assert_ne!(PAYLOAD_MODE_HTTP, PAYLOAD_MODE_TUNNEL);
        assert_eq!(PAYLOAD_MODE_HTTP, 0x00);
        assert_eq!(PAYLOAD_MODE_TUNNEL, 0x01);
    }
}

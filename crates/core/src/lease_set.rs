//! Lease sets for anonymous response routing
//!
//! A LeaseSet contains gateway entries that the exit can use to route
//! response shards back to the client without learning the client's identity.
//! Each Lease points to a gateway relay with a pre-negotiated tunnel_id.

use serde::{Deserialize, Serialize};

use crate::types::Id;

/// Collection of gateway leases for response routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseSet {
    /// Session identifier (client-generated, unique per session)
    pub session_id: Id,
    /// Available gateway leases
    pub leases: Vec<Lease>,
}

/// A single gateway lease entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    /// PeerId of the gateway relay (as bytes)
    pub gateway_peer_id: Vec<u8>,
    /// Gateway's X25519 encryption pubkey (for onion header construction)
    pub gateway_encryption_pubkey: [u8; 32],
    /// Pre-negotiated tunnel ID (registered with gateway)
    pub tunnel_id: Id,
    /// Unix timestamp when this lease expires
    pub expires_at: u64,
}

impl LeaseSet {
    /// Create a new empty lease set
    pub fn new(session_id: Id) -> Self {
        Self {
            session_id,
            leases: Vec::new(),
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lease_set_new() {
        let ls = LeaseSet::new([1u8; 32]);
        assert_eq!(ls.session_id, [1u8; 32]);
        assert!(ls.leases.is_empty());
    }

    #[test]
    fn test_lease_set_serde() {
        let ls = LeaseSet {
            session_id: [1u8; 32],
            leases: vec![
                Lease {
                    gateway_peer_id: vec![10, 20, 30],
                    gateway_encryption_pubkey: [2u8; 32],
                    tunnel_id: [3u8; 32],
                    expires_at: 1000,
                },
                Lease {
                    gateway_peer_id: vec![40, 50],
                    gateway_encryption_pubkey: [4u8; 32],
                    tunnel_id: [5u8; 32],
                    expires_at: 2000,
                },
            ],
        };

        let bytes = ls.to_bytes().unwrap();
        let restored = LeaseSet::from_bytes(&bytes).unwrap();
        assert_eq!(restored.session_id, [1u8; 32]);
        assert_eq!(restored.leases.len(), 2);
        assert_eq!(restored.leases[0].gateway_peer_id, vec![10, 20, 30]);
        assert_eq!(restored.leases[0].tunnel_id, [3u8; 32]);
        assert_eq!(restored.leases[1].expires_at, 2000);
    }

    #[test]
    fn test_empty_lease_set_serde() {
        let ls = LeaseSet::new([0u8; 32]);
        let bytes = ls.to_bytes().unwrap();
        let restored = LeaseSet::from_bytes(&bytes).unwrap();
        assert!(restored.leases.is_empty());
    }
}

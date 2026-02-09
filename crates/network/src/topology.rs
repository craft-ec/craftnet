//! Topology gossip for relay/exit encryption key advertisement and connectivity
//!
//! Relays and exits publish TopologyMessages to advertise:
//! - Their X25519 encryption public key (for onion routing)
//! - Their connected peers (for path selection)
//!
//! Clients subscribe to the topology topic and build a TopologyGraph
//! for selecting valid multi-hop onion paths.

use serde::{Deserialize, Serialize};

/// Gossipsub topic for topology advertisements
pub const TOPOLOGY_TOPIC: &str = "tunnelcraft/topology/1.0.0";

/// Topology advertisement broadcast via gossipsub
///
/// Published on: peer connect, peer disconnect, periodic heartbeat (60s).
/// Signed by the advertising node's Ed25519 key to prevent spoofing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyMessage {
    /// Hex-encoded signing pubkey (Ed25519)
    pub pubkey: String,
    /// libp2p PeerId string
    pub peer_id: String,
    /// Hex-encoded X25519 encryption pubkey (for onion ECDH)
    pub encryption_pubkey: String,
    /// PeerId strings of currently connected peers
    pub connected_peers: Vec<String>,
    /// Unix timestamp (seconds)
    pub timestamp: u64,
    /// Ed25519 signature over signable fields
    pub signature: Vec<u8>,
}

impl TopologyMessage {
    /// Create a new topology message (unsigned — caller must set signature)
    pub fn new(
        pubkey: [u8; 32],
        peer_id: &str,
        encryption_pubkey: [u8; 32],
        connected_peers: Vec<String>,
    ) -> Self {
        Self {
            pubkey: hex::encode(pubkey),
            peer_id: peer_id.to_string(),
            encryption_pubkey: hex::encode(encryption_pubkey),
            connected_peers,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            signature: Vec::new(),
        }
    }

    /// Get the data to sign: pubkey || peer_id || encryption_pubkey || connected_peers (sorted) || timestamp
    pub fn signable_data(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(self.pubkey.as_bytes());
        data.push(b'|');
        data.extend_from_slice(self.peer_id.as_bytes());
        data.push(b'|');
        data.extend_from_slice(self.encryption_pubkey.as_bytes());
        data.push(b'|');
        let mut sorted_peers = self.connected_peers.clone();
        sorted_peers.sort();
        for peer in &sorted_peers {
            data.extend_from_slice(peer.as_bytes());
            data.push(b',');
        }
        data.push(b'|');
        data.extend_from_slice(&self.timestamp.to_le_bytes());
        data
    }

    /// Serialize to JSON bytes for gossipsub
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Parse from JSON bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }

    /// Get signing pubkey as bytes
    pub fn pubkey_bytes(&self) -> Option<[u8; 32]> {
        let bytes = hex::decode(&self.pubkey).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(arr)
    }

    /// Get encryption pubkey as bytes
    pub fn encryption_pubkey_bytes(&self) -> Option<[u8; 32]> {
        let bytes = hex::decode(&self.encryption_pubkey).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topology_message_creation() {
        let msg = TopologyMessage::new(
            [1u8; 32],
            "12D3KooWTest",
            [2u8; 32],
            vec!["peer_a".to_string(), "peer_b".to_string()],
        );

        assert_eq!(msg.pubkey, hex::encode([1u8; 32]));
        assert_eq!(msg.peer_id, "12D3KooWTest");
        assert_eq!(msg.encryption_pubkey, hex::encode([2u8; 32]));
        assert_eq!(msg.connected_peers.len(), 2);
        assert!(msg.timestamp > 0);
    }

    #[test]
    fn test_serde_roundtrip() {
        let msg = TopologyMessage::new(
            [3u8; 32],
            "peer123",
            [4u8; 32],
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        );

        let bytes = msg.to_bytes();
        let parsed = TopologyMessage::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.pubkey, msg.pubkey);
        assert_eq!(parsed.peer_id, msg.peer_id);
        assert_eq!(parsed.encryption_pubkey, msg.encryption_pubkey);
        assert_eq!(parsed.connected_peers, msg.connected_peers);
        assert_eq!(parsed.timestamp, msg.timestamp);
    }

    #[test]
    fn test_pubkey_bytes() {
        let pk = [5u8; 32];
        let msg = TopologyMessage::new(pk, "peer", [0u8; 32], vec![]);
        assert_eq!(msg.pubkey_bytes(), Some(pk));
    }

    #[test]
    fn test_encryption_pubkey_bytes() {
        let ek = [6u8; 32];
        let msg = TopologyMessage::new([0u8; 32], "peer", ek, vec![]);
        assert_eq!(msg.encryption_pubkey_bytes(), Some(ek));
    }

    #[test]
    fn test_signable_data_deterministic() {
        let msg = TopologyMessage::new(
            [1u8; 32],
            "peer1",
            [2u8; 32],
            vec!["b".to_string(), "a".to_string()],
        );
        let data1 = msg.signable_data();
        let data2 = msg.signable_data();
        assert_eq!(data1, data2);
    }

    #[test]
    fn test_signable_data_sorted_peers() {
        let mut msg1 = TopologyMessage::new(
            [1u8; 32],
            "peer1",
            [2u8; 32],
            vec!["b".to_string(), "a".to_string()],
        );
        msg1.timestamp = 100;

        let mut msg2 = TopologyMessage::new(
            [1u8; 32],
            "peer1",
            [2u8; 32],
            vec!["a".to_string(), "b".to_string()],
        );
        msg2.timestamp = 100;

        // Sorted peers → same signable data regardless of input order
        assert_eq!(msg1.signable_data(), msg2.signable_data());
    }

    #[test]
    fn test_topology_topic() {
        assert_eq!(TOPOLOGY_TOPIC, "tunnelcraft/topology/1.0.0");
    }

    #[test]
    fn test_empty_connected_peers() {
        let msg = TopologyMessage::new([0u8; 32], "peer", [0u8; 32], vec![]);
        assert!(msg.connected_peers.is_empty());

        let bytes = msg.to_bytes();
        let parsed = TopologyMessage::from_bytes(&bytes).unwrap();
        assert!(parsed.connected_peers.is_empty());
    }
}

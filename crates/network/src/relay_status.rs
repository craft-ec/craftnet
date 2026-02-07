//! Relay node status messages for gossipsub
//!
//! Lightweight real-time updates for relay node status:
//! - Heartbeat: "I'm alive" with load, queue depth, and bandwidth info
//! - Offline: graceful shutdown announcement
//!
//! Relays announce their self-reported capacity. Clients score relays
//! using a weighted formula over load, queue, bandwidth, and uptime.

use serde::{Deserialize, Serialize};

/// Relay status event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayStatusType {
    /// Periodic heartbeat (relay is online and healthy)
    Heartbeat,
    /// Relay is going offline (graceful shutdown)
    Offline,
}

/// Relay status message broadcast via gossipsub
///
/// Contains load and throughput information for relay selection scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayStatusMessage {
    /// Type of status update
    pub status: RelayStatusType,
    /// Relay node's public key (32 bytes, hex encoded)
    pub pubkey: String,
    /// Relay node's peer ID
    pub peer_id: String,
    /// Current load percentage (0-100)
    pub load_percent: u8,
    /// Number of active connections being relayed
    pub active_connections: u32,
    /// Current queue depth (shards waiting to be forwarded)
    pub queue_depth: u32,
    /// Available bandwidth in KB/s
    pub bandwidth_available_kbps: u32,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Unix timestamp (seconds)
    pub timestamp: u64,
}

impl RelayStatusMessage {
    /// Create a heartbeat message
    pub fn heartbeat(
        pubkey: [u8; 32],
        peer_id: &str,
        load_percent: u8,
        active_connections: u32,
        queue_depth: u32,
        bandwidth_available_kbps: u32,
        uptime_secs: u64,
    ) -> Self {
        Self {
            status: RelayStatusType::Heartbeat,
            pubkey: hex::encode(pubkey),
            peer_id: peer_id.to_string(),
            load_percent: load_percent.min(100),
            active_connections,
            queue_depth,
            bandwidth_available_kbps,
            uptime_secs,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Create an offline announcement
    pub fn offline(pubkey: [u8; 32], peer_id: &str) -> Self {
        Self {
            status: RelayStatusType::Offline,
            pubkey: hex::encode(pubkey),
            peer_id: peer_id.to_string(),
            load_percent: 0,
            active_connections: 0,
            queue_depth: 0,
            bandwidth_available_kbps: 0,
            uptime_secs: 0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Serialize to JSON bytes for gossipsub
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Parse from JSON bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }

    /// Get pubkey as bytes
    pub fn pubkey_bytes(&self) -> Option<[u8; 32]> {
        let bytes = hex::decode(&self.pubkey).ok()?;
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
    fn test_heartbeat_message() {
        let msg = RelayStatusMessage::heartbeat(
            [1u8; 32],
            "12D3KooW...",
            65,
            42,
            100,
            50000,
            3600,
        );

        assert_eq!(msg.status, RelayStatusType::Heartbeat);
        assert_eq!(msg.load_percent, 65);
        assert_eq!(msg.active_connections, 42);
        assert_eq!(msg.queue_depth, 100);
        assert_eq!(msg.bandwidth_available_kbps, 50000);
        assert_eq!(msg.uptime_secs, 3600);
    }

    #[test]
    fn test_offline_message() {
        let msg = RelayStatusMessage::offline([2u8; 32], "12D3KooW...");

        assert_eq!(msg.status, RelayStatusType::Offline);
        assert_eq!(msg.load_percent, 0);
        assert_eq!(msg.queue_depth, 0);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let msg = RelayStatusMessage::heartbeat(
            [3u8; 32],
            "peer123",
            50,
            10,
            25,
            25000,
            86400,
        );
        let bytes = msg.to_bytes();
        let parsed = RelayStatusMessage::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.status, msg.status);
        assert_eq!(parsed.pubkey, msg.pubkey);
        assert_eq!(parsed.load_percent, msg.load_percent);
        assert_eq!(parsed.active_connections, msg.active_connections);
        assert_eq!(parsed.queue_depth, msg.queue_depth);
        assert_eq!(parsed.bandwidth_available_kbps, msg.bandwidth_available_kbps);
        assert_eq!(parsed.uptime_secs, msg.uptime_secs);
    }

    #[test]
    fn test_load_clamped_to_100() {
        let msg = RelayStatusMessage::heartbeat([4u8; 32], "peer", 150, 0, 0, 0, 0);
        assert_eq!(msg.load_percent, 100);
    }

    #[test]
    fn test_pubkey_bytes() {
        let pubkey = [5u8; 32];
        let msg = RelayStatusMessage::heartbeat(pubkey, "peer", 0, 0, 0, 0, 0);
        assert_eq!(msg.pubkey_bytes(), Some(pubkey));
    }
}

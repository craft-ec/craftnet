//! Exit node status messages for gossipsub
//!
//! Lightweight real-time updates for exit node status:
//! - Heartbeat: "I'm alive" with load and throughput info
//! - Offline: graceful shutdown announcement
//!
//! Exits announce their self-reported capacity. Clients measure actual
//! throughput and compare against announced values for trust scoring.

use serde::{Deserialize, Serialize};

/// Exit status event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitStatusType {
    /// Periodic heartbeat (exit is online and healthy)
    Heartbeat,
    /// Exit is going offline (graceful shutdown)
    Offline,
}

/// Exit status message broadcast via gossipsub
///
/// Contains both load and throughput information for exit selection.
/// New exits start with base 50% score; measurements adjust over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitStatusMessage {
    /// Type of status update
    pub status: ExitStatusType,
    /// Exit node's public key (32 bytes, hex encoded)
    pub pubkey: String,
    /// Exit node's peer ID
    pub peer_id: String,
    /// Current load percentage (0-100)
    pub load_percent: u8,
    /// Number of active connections
    pub active_connections: u32,
    /// Self-reported uplink capacity (KB/s)
    pub uplink_kbps: u32,
    /// Self-reported downlink capacity (KB/s)
    pub downlink_kbps: u32,
    /// Self-reported uptime in seconds (how long exit has been running)
    pub uptime_secs: u64,
    /// Optional region hint (e.g., "us-west", "eu-central")
    pub region: Option<String>,
    /// Unix timestamp (seconds)
    pub timestamp: u64,
}

impl ExitStatusMessage {
    /// Create a heartbeat message with throughput info
    pub fn heartbeat(
        pubkey: [u8; 32],
        peer_id: &str,
        load_percent: u8,
        active_connections: u32,
        uplink_kbps: u32,
        downlink_kbps: u32,
        uptime_secs: u64,
        region: Option<String>,
    ) -> Self {
        Self {
            status: ExitStatusType::Heartbeat,
            pubkey: hex::encode(pubkey),
            peer_id: peer_id.to_string(),
            load_percent: load_percent.min(100),
            active_connections,
            uplink_kbps,
            downlink_kbps,
            uptime_secs,
            region,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Create an offline announcement
    pub fn offline(pubkey: [u8; 32], peer_id: &str) -> Self {
        Self {
            status: ExitStatusType::Offline,
            pubkey: hex::encode(pubkey),
            peer_id: peer_id.to_string(),
            load_percent: 0,
            active_connections: 0,
            uplink_kbps: 0,
            downlink_kbps: 0,
            uptime_secs: 0,
            region: None,
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
        let msg = ExitStatusMessage::heartbeat(
            [1u8; 32],
            "12D3KooW...",
            65,
            42,
            10000,  // 10 MB/s uplink
            50000,  // 50 MB/s downlink
            3600,   // 1 hour uptime
            Some("us-west".to_string()),
        );

        assert_eq!(msg.status, ExitStatusType::Heartbeat);
        assert_eq!(msg.load_percent, 65);
        assert_eq!(msg.active_connections, 42);
        assert_eq!(msg.uplink_kbps, 10000);
        assert_eq!(msg.downlink_kbps, 50000);
        assert_eq!(msg.uptime_secs, 3600);
        assert_eq!(msg.region, Some("us-west".to_string()));
    }

    #[test]
    fn test_offline_message() {
        let msg = ExitStatusMessage::offline([2u8; 32], "12D3KooW...");

        assert_eq!(msg.status, ExitStatusType::Offline);
        assert_eq!(msg.load_percent, 0);
        assert_eq!(msg.uplink_kbps, 0);
        assert_eq!(msg.downlink_kbps, 0);
        assert_eq!(msg.uptime_secs, 0);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let msg = ExitStatusMessage::heartbeat(
            [3u8; 32],
            "peer123",
            50,
            10,
            5000,
            25000,
            86400,  // 1 day uptime
            Some("eu-central".to_string()),
        );
        let bytes = msg.to_bytes();
        let parsed = ExitStatusMessage::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.status, msg.status);
        assert_eq!(parsed.pubkey, msg.pubkey);
        assert_eq!(parsed.load_percent, msg.load_percent);
        assert_eq!(parsed.uplink_kbps, msg.uplink_kbps);
        assert_eq!(parsed.downlink_kbps, msg.downlink_kbps);
        assert_eq!(parsed.uptime_secs, msg.uptime_secs);
        assert_eq!(parsed.region, msg.region);
    }

    #[test]
    fn test_load_clamped_to_100() {
        let msg = ExitStatusMessage::heartbeat([4u8; 32], "peer", 150, 0, 0, 0, 0, None);
        assert_eq!(msg.load_percent, 100);
    }

    #[test]
    fn test_pubkey_bytes() {
        let pubkey = [5u8; 32];
        let msg = ExitStatusMessage::heartbeat(pubkey, "peer", 0, 0, 0, 0, 0, None);
        assert_eq!(msg.pubkey_bytes(), Some(pubkey));
    }
}

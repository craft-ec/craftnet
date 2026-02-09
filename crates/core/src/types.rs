use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// 32-byte identifier
pub type Id = [u8; 32];

/// 32-byte public key
pub type PublicKey = [u8; 32];

/// 64-byte signature (use BigArray for serde support)
pub type Signature = [u8; 64];

/// Subscription tier for users
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionTier {
    /// 10 GB / month
    Basic,
    /// 100 GB / month
    Standard,
    /// 1 TB + best-effort beyond / month
    Premium,
}

/// A single entry in the signature chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainEntry {
    /// Public key of the node that signed
    pub pubkey: PublicKey,
    /// Signature over the shard data
    #[serde(with = "BigArray")]
    pub signature: Signature,
    /// Hops remaining at the time of signing (needed for verification)
    pub hops_at_sign: u8,
}

impl ChainEntry {
    pub fn new(pubkey: PublicKey, signature: Signature, hops_at_sign: u8) -> Self {
        Self { pubkey, signature, hops_at_sign }
    }
}

/// Minimum hop count for privacy levels.
///
/// Every path always starts with a gateway relay (base 1). There is no
/// direct client → exit connection. The hop count includes the gateway.
///
/// | Mode     | Total hops | Path                                        |
/// |----------|-----------|---------------------------------------------|
/// | Direct   | 1         | client → gateway → exit                     |
/// | Single | 1         | client → gateway → exit                       |
/// | Double | 2         | client → gateway → relay → exit               |
/// | Triple | 3         | client → gateway → relay → relay → exit       |
/// | Quad   | 4         | client → gateway → relay → relay → relay → exit |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HopMode {
    /// 1 hop (gateway only) - fastest, exit sees gateway IP
    Single,
    /// 2 hops - basic privacy, no single node sees both client and exit
    Double,
    /// 3 hops - good privacy (default)
    Triple,
    /// 4 hops - maximum privacy
    Quad,
}

impl HopMode {
    /// Total number of relay hops (including the gateway).
    /// Always >= 1. The gateway is always present.
    pub fn min_relays(&self) -> u8 {
        match self {
            HopMode::Single => 1,
            HopMode::Double => 2,
            HopMode::Triple => 3,
            HopMode::Quad => 4,
        }
    }

    /// Extra relay hops beyond the gateway.
    /// Single=0, Double=1, Triple=2, Quad=3.
    pub fn extra_hops(&self) -> u8 {
        self.min_relays() - 1
    }

    /// Deprecated: use `min_relays()` instead.
    pub fn hop_count(&self) -> u8 {
        self.min_relays()
    }

    pub fn from_count(count: u8) -> Self {
        match count {
            0 | 1 => HopMode::Single,
            2 => HopMode::Double,
            3 => HopMode::Triple,
            _ => HopMode::Quad,
        }
    }
}

/// Geographic region for exit nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExitRegion {
    /// Automatic selection based on latency
    #[default]
    Auto,
    /// North America
    NorthAmerica,
    /// Europe
    Europe,
    /// Asia Pacific
    AsiaPacific,
    /// South America
    SouthAmerica,
    /// Africa
    Africa,
    /// Middle East
    MiddleEast,
    /// Oceania (Australia, New Zealand)
    Oceania,
}

impl ExitRegion {
    /// Get display name for the region
    pub fn display_name(&self) -> &'static str {
        match self {
            ExitRegion::Auto => "Auto",
            ExitRegion::NorthAmerica => "North America",
            ExitRegion::Europe => "Europe",
            ExitRegion::AsiaPacific => "Asia Pacific",
            ExitRegion::SouthAmerica => "South America",
            ExitRegion::Africa => "Africa",
            ExitRegion::MiddleEast => "Middle East",
            ExitRegion::Oceania => "Oceania",
        }
    }

    /// Get short code for the region
    pub fn code(&self) -> &'static str {
        match self {
            ExitRegion::Auto => "auto",
            ExitRegion::NorthAmerica => "na",
            ExitRegion::Europe => "eu",
            ExitRegion::AsiaPacific => "ap",
            ExitRegion::SouthAmerica => "sa",
            ExitRegion::Africa => "af",
            ExitRegion::MiddleEast => "me",
            ExitRegion::Oceania => "oc",
        }
    }

    /// Get flag emoji for the region
    pub fn flag(&self) -> &'static str {
        match self {
            ExitRegion::Auto => "🌍",
            ExitRegion::NorthAmerica => "🇺🇸",
            ExitRegion::Europe => "🇪🇺",
            ExitRegion::AsiaPacific => "🇯🇵",
            ExitRegion::SouthAmerica => "🇧🇷",
            ExitRegion::Africa => "🇿🇦",
            ExitRegion::MiddleEast => "🇦🇪",
            ExitRegion::Oceania => "🇦🇺",
        }
    }
}

/// Information about an exit node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitInfo {
    pub pubkey: PublicKey,
    pub address: String,
    pub region: ExitRegion,
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub reputation: u64,
    pub latency_ms: u32,
    /// X25519 encryption pubkey (for onion routing)
    #[serde(default)]
    pub encryption_pubkey: Option<[u8; 32]>,
    /// libp2p PeerId string (learned from gossipsub or DHT)
    #[serde(default)]
    pub peer_id: Option<String>,
}

/// Information about a relay node (stored in DHT)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayInfo {
    pub pubkey: PublicKey,
    pub address: String,
    pub allows_last_hop: bool,
    pub reputation: u64,
    /// X25519 encryption pubkey (for onion routing)
    #[serde(default)]
    pub encryption_pubkey: Option<[u8; 32]>,
}

/// Information about a peer node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub pubkey: PublicKey,
    pub address: String,
    pub is_exit: bool,
}


/// Cryptographic receipt proving a relay received and will forward a shard.
///
/// When relay A sends a shard to relay B, relay B signs a receipt proving
/// delivery. Relay A uses this receipt as on-chain proof for settlement.
/// This replaces TCP ACK (which is fakeable at the transport level).
///
/// The `blind_token` field is a per-hop unique derivation:
/// `blind_token = SHA256(user_proof || shard_id || relay_pubkey)`.
/// Each relay sees a different blind_token, preventing colluding relays
/// from correlating receipts across the same path.
///
/// The `epoch` field prevents cross-epoch receipt replay: a relay cannot
/// resubmit the same receipts to future subscription epochs for double rewards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardReceipt {
    /// Request this shard belongs to
    pub request_id: Id,
    /// Per-hop unique shard identifier (includes relay_pubkey in derivation)
    pub shard_id: Id,
    /// Public key of the relay that forwarded the shard (anti-Sybil: binds receipt to sender)
    pub sender_pubkey: PublicKey,
    /// Public key of the receiving node (signs this receipt)
    pub receiver_pubkey: PublicKey,
    /// Per-hop unique blind token: SHA256(user_proof || shard_id || relay_pubkey)
    /// Prevents cross-relay correlation of settlement data.
    pub blind_token: Id,
    /// Actual payload bytes in the forwarded shard.
    /// Settlement weights rewards by bandwidth, not receipt count.
    pub payload_size: u32,
    /// Subscription epoch this receipt belongs to (prevents cross-epoch replay)
    pub epoch: u64,
    /// Unix timestamp (seconds) when the shard was received
    pub timestamp: u64,
    /// Receiver's ed25519 signature over the receipt payload
    #[serde(with = "BigArray")]
    pub signature: Signature,
}

impl ForwardReceipt {
    /// Get the data that the receiver signs (180 bytes):
    /// request_id(32) || shard_id(32) || sender_pubkey(32) || receiver_pubkey(32) || blind_token(32) || payload_size_le(4) || epoch_le(8) || timestamp_le(8)
    #[allow(clippy::too_many_arguments)]
    pub fn signable_data(
        request_id: &Id,
        shard_id: &Id,
        sender_pubkey: &PublicKey,
        receiver_pubkey: &PublicKey,
        blind_token: &Id,
        payload_size: u32,
        epoch: u64,
        timestamp: u64,
    ) -> Vec<u8> {
        let mut data = Vec::with_capacity(32 + 32 + 32 + 32 + 32 + 4 + 8 + 8);
        data.extend_from_slice(request_id);
        data.extend_from_slice(shard_id);
        data.extend_from_slice(sender_pubkey);
        data.extend_from_slice(receiver_pubkey);
        data.extend_from_slice(blind_token);
        data.extend_from_slice(&payload_size.to_le_bytes());
        data.extend_from_slice(&epoch.to_le_bytes());
        data.extend_from_slice(&timestamp.to_le_bytes());
        data
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // ==================== HopMode Tests ====================

    #[test]
    fn test_hop_mode_min_relays() {
        assert_eq!(HopMode::Single.min_relays(), 1);
        assert_eq!(HopMode::Double.min_relays(), 2);
        assert_eq!(HopMode::Triple.min_relays(), 3);
        assert_eq!(HopMode::Quad.min_relays(), 4);
    }

    #[test]
    fn test_hop_mode_extra_hops() {
        assert_eq!(HopMode::Single.extra_hops(), 0);
        assert_eq!(HopMode::Double.extra_hops(), 1);
        assert_eq!(HopMode::Triple.extra_hops(), 2);
        assert_eq!(HopMode::Quad.extra_hops(), 3);
    }

    #[test]
    fn test_hop_mode_hop_count_compat() {
        // hop_count() is a deprecated alias for min_relays()
        assert_eq!(HopMode::Single.hop_count(), HopMode::Single.min_relays());
        assert_eq!(HopMode::Quad.hop_count(), HopMode::Quad.min_relays());
    }

    #[test]
    fn test_hop_mode_from_count() {
        assert_eq!(HopMode::from_count(0), HopMode::Single);
        assert_eq!(HopMode::from_count(1), HopMode::Single);
        assert_eq!(HopMode::from_count(2), HopMode::Double);
        assert_eq!(HopMode::from_count(3), HopMode::Triple);
    }

    #[test]
    fn test_hop_mode_from_count_high_values() {
        // Any value >= 4 should map to Quad
        assert_eq!(HopMode::from_count(4), HopMode::Quad);
        assert_eq!(HopMode::from_count(10), HopMode::Quad);
        assert_eq!(HopMode::from_count(255), HopMode::Quad);
    }

    #[test]
    fn test_hop_mode_roundtrip() {
        for mode in [HopMode::Single, HopMode::Double, HopMode::Triple, HopMode::Quad] {
            let count = mode.min_relays();
            assert_eq!(HopMode::from_count(count), mode);
        }
    }

    #[test]
    fn test_hop_mode_equality() {
        assert_eq!(HopMode::Single, HopMode::Single);
        assert_ne!(HopMode::Single, HopMode::Double);
        assert_ne!(HopMode::Double, HopMode::Triple);
        assert_ne!(HopMode::Triple, HopMode::Quad);
    }

    // ==================== ChainEntry Tests ====================

    #[test]
    fn test_chain_entry_creation() {
        let entry = ChainEntry::new([1u8; 32], [2u8; 64], 3);

        assert_eq!(entry.pubkey, [1u8; 32]);
        assert_eq!(entry.signature, [2u8; 64]);
        assert_eq!(entry.hops_at_sign, 3);
    }

    #[test]
    fn test_chain_entry_zero_hops() {
        let entry = ChainEntry::new([1u8; 32], [0u8; 64], 0);
        assert_eq!(entry.hops_at_sign, 0);
    }

    #[test]
    fn test_chain_entry_max_hops() {
        let entry = ChainEntry::new([1u8; 32], [0u8; 64], 255);
        assert_eq!(entry.hops_at_sign, 255);
    }

    // ==================== SubscriptionTier Tests ====================

    #[test]
    fn test_subscription_tier_equality() {
        assert_eq!(SubscriptionTier::Basic, SubscriptionTier::Basic);
        assert_eq!(SubscriptionTier::Standard, SubscriptionTier::Standard);
        assert_eq!(SubscriptionTier::Premium, SubscriptionTier::Premium);
        assert_ne!(SubscriptionTier::Basic, SubscriptionTier::Standard);
        assert_ne!(SubscriptionTier::Standard, SubscriptionTier::Premium);
    }

    #[test]
    fn test_subscription_tier_serialization() {
        for tier in [SubscriptionTier::Basic, SubscriptionTier::Standard, SubscriptionTier::Premium] {
            let json = serde_json::to_string(&tier).unwrap();
            let restored: SubscriptionTier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, restored);
        }
    }

    // ==================== ForwardReceipt Tests ====================

    #[test]
    fn test_forward_receipt_signable_data() {
        let request_id = [1u8; 32];
        let shard_id = [3u8; 32];
        let sender_pubkey = [5u8; 32];
        let receiver_pubkey = [2u8; 32];
        let blind_token = [4u8; 32];
        let data = ForwardReceipt::signable_data(&request_id, &shard_id, &sender_pubkey, &receiver_pubkey, &blind_token, 1024, 42, 1000);

        // 32 (request_id) + 32 (shard_id) + 32 (sender_pubkey) + 32 (receiver_pubkey) + 32 (user_proof) + 4 (payload_size) + 8 (epoch) + 8 (timestamp) = 180
        assert_eq!(data.len(), 180);
        assert_eq!(&data[0..32], &request_id);
        assert_eq!(&data[32..64], &shard_id);
        assert_eq!(&data[64..96], &sender_pubkey);
        assert_eq!(&data[96..128], &receiver_pubkey);
        assert_eq!(&data[128..160], &blind_token);
        assert_eq!(&data[160..164], &1024u32.to_le_bytes());
        assert_eq!(&data[164..172], &42u64.to_le_bytes());
        assert_eq!(&data[172..180], &1000u64.to_le_bytes());
    }

    #[test]
    fn test_forward_receipt_signable_data_different_inputs() {
        let blind_token = [5u8; 32];
        let sender = [9u8; 32];
        let data1 = ForwardReceipt::signable_data(&[1u8; 32], &[10u8; 32], &sender, &[2u8; 32], &blind_token, 1024, 0, 100);
        let data2 = ForwardReceipt::signable_data(&[1u8; 32], &[11u8; 32], &sender, &[2u8; 32], &blind_token, 1024, 0, 100);
        // Different shard_id should produce different data
        assert_ne!(data1, data2);
    }

    #[test]
    fn test_forward_receipt_same_relay_different_shards() {
        // Same relay, same request — but different shard_ids (e.g. request vs response shard)
        let blind_token = [5u8; 32];
        let sender = [9u8; 32];
        let data1 = ForwardReceipt::signable_data(&[1u8; 32], &[10u8; 32], &sender, &[2u8; 32], &blind_token, 1024, 0, 100);
        let data2 = ForwardReceipt::signable_data(&[1u8; 32], &[20u8; 32], &sender, &[2u8; 32], &blind_token, 1024, 0, 100);
        assert_ne!(data1, data2);
    }

    #[test]
    fn test_forward_receipt_different_blind_tokens() {
        let sender = [9u8; 32];
        let data1 = ForwardReceipt::signable_data(&[1u8; 32], &[10u8; 32], &sender, &[2u8; 32], &[5u8; 32], 1024, 0, 100);
        let data2 = ForwardReceipt::signable_data(&[1u8; 32], &[10u8; 32], &sender, &[2u8; 32], &[6u8; 32], 1024, 0, 100);
        assert_ne!(data1, data2);
    }

    #[test]
    fn test_forward_receipt_different_senders() {
        let blind_token = [5u8; 32];
        let data1 = ForwardReceipt::signable_data(&[1u8; 32], &[10u8; 32], &[9u8; 32], &[2u8; 32], &blind_token, 1024, 0, 100);
        let data2 = ForwardReceipt::signable_data(&[1u8; 32], &[10u8; 32], &[8u8; 32], &[2u8; 32], &blind_token, 1024, 0, 100);
        assert_ne!(data1, data2, "Different senders should produce different signable data");
    }

    #[test]
    fn test_forward_receipt_different_epochs() {
        let blind_token = [5u8; 32];
        let sender = [9u8; 32];
        let data1 = ForwardReceipt::signable_data(&[1u8; 32], &[10u8; 32], &sender, &[2u8; 32], &blind_token, 1024, 0, 100);
        let data2 = ForwardReceipt::signable_data(&[1u8; 32], &[10u8; 32], &sender, &[2u8; 32], &blind_token, 1024, 1, 100);
        assert_ne!(data1, data2, "Different epochs should produce different signable data");
    }

    // ==================== ExitInfo Tests ====================

    #[test]
    fn test_exit_info_creation() {
        let exit = ExitInfo {
            pubkey: [1u8; 32],
            address: "exit.example.com:9000".to_string(),
            region: ExitRegion::NorthAmerica,
            country_code: Some("US".to_string()),
            city: Some("New York".to_string()),
            reputation: 100,
            latency_ms: 50,
            encryption_pubkey: None,
            peer_id: None,
        };

        assert_eq!(exit.pubkey, [1u8; 32]);
        assert_eq!(exit.address, "exit.example.com:9000");
        assert_eq!(exit.region, ExitRegion::NorthAmerica);
        assert_eq!(exit.country_code, Some("US".to_string()));
        assert_eq!(exit.city, Some("New York".to_string()));
        assert_eq!(exit.reputation, 100);
        assert_eq!(exit.latency_ms, 50);
    }

    #[test]
    fn test_exit_info_zero_values() {
        let exit = ExitInfo {
            pubkey: [0u8; 32],
            address: String::new(),
            region: ExitRegion::Auto,
            country_code: None,
            city: None,
            reputation: 0,
            latency_ms: 0,
            encryption_pubkey: None,
            peer_id: None,
        };

        assert!(exit.address.is_empty());
        assert_eq!(exit.region, ExitRegion::Auto);
        assert_eq!(exit.reputation, 0);
    }

    // ==================== PeerInfo Tests ====================

    #[test]
    fn test_peer_info_exit() {
        let peer = PeerInfo {
            pubkey: [1u8; 32],
            address: "peer.example.com:8000".to_string(),
            is_exit: true,
        };

        assert!(peer.is_exit);
    }

    #[test]
    fn test_peer_info_relay() {
        let peer = PeerInfo {
            pubkey: [1u8; 32],
            address: "relay.example.com:8000".to_string(),
            is_exit: false,
        };

        assert!(!peer.is_exit);
    }

    // ==================== Serialization Tests ====================

    #[test]
    fn test_hop_mode_serialization() {
        let mode = HopMode::Triple;
        let json = serde_json::to_string(&mode).unwrap();
        let restored: HopMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, restored);
    }

    #[test]
    fn test_chain_entry_serialization() {
        let entry = ChainEntry::new([1u8; 32], [2u8; 64], 3);
        let json = serde_json::to_string(&entry).unwrap();
        let restored: ChainEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.pubkey, restored.pubkey);
        assert_eq!(entry.signature, restored.signature);
        assert_eq!(entry.hops_at_sign, restored.hops_at_sign);
    }

    #[test]
    fn test_exit_info_serialization() {
        let exit = ExitInfo {
            pubkey: [1u8; 32],
            address: "test.com:9000".to_string(),
            region: ExitRegion::Europe,
            country_code: Some("DE".to_string()),
            city: Some("Frankfurt".to_string()),
            reputation: 50,
            latency_ms: 100,
            encryption_pubkey: None,
            peer_id: None,
        };

        let json = serde_json::to_string(&exit).unwrap();
        let restored: ExitInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(exit.pubkey, restored.pubkey);
        assert_eq!(exit.address, restored.address);
        assert_eq!(exit.region, restored.region);
        assert_eq!(exit.country_code, restored.country_code);
        assert_eq!(exit.city, restored.city);
        assert_eq!(exit.reputation, restored.reputation);
        assert_eq!(exit.latency_ms, restored.latency_ms);
    }

}

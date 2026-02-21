use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use bitflags::bitflags;

bitflags! {
    /// Composable node capabilities.
    ///
    /// Each capability is independent and can be combined freely:
    /// - `CLIENT`     — Route personal VPN traffic (spend credits)
    /// - `RELAY`      — Forward shards for others (earn credits)
    /// - `EXIT`       — Execute requests at edge (earn credits)
    /// - `AGGREGATOR` — Collect proofs, build distributions
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Capabilities: u8 {
        /// Route personal VPN traffic
        const CLIENT     = 0b0001;
        /// Forward shards for others
        const RELAY      = 0b0010;
        /// Execute requests at edge
        const EXIT       = 0b0100;
        /// Collect proofs, build distributions
        const AGGREGATOR = 0b1000;
    }
}

impl Capabilities {
    /// Whether this node routes personal VPN traffic.
    pub fn is_client(self) -> bool {
        self.contains(Capabilities::CLIENT)
    }

    /// Whether this node provides any service (relay, exit, or aggregator).
    pub fn is_service_node(self) -> bool {
        self.intersects(Capabilities::RELAY | Capabilities::EXIT | Capabilities::AGGREGATOR)
    }

    /// Whether this node relays shards for others.
    pub fn is_relay(self) -> bool {
        self.contains(Capabilities::RELAY)
    }

    /// Whether this node acts as an exit.
    pub fn is_exit(self) -> bool {
        self.contains(Capabilities::EXIT)
    }

    /// Whether this node runs the aggregator.
    pub fn is_aggregator(self) -> bool {
        self.contains(Capabilities::AGGREGATOR)
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities::CLIENT
    }
}

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
    /// Unlimited / month — maximum privacy (4 hops)
    Ultra,
}

impl SubscriptionTier {
    /// Maximum hop mode allowed for this tier.
    /// Basic=1 hop, Standard=2 hops, Premium=3 hops, Ultra=4 hops.
    pub fn max_hop_mode(&self) -> HopMode {
        match self {
            SubscriptionTier::Basic => HopMode::Single,
            SubscriptionTier::Standard => HopMode::Double,
            SubscriptionTier::Premium => HopMode::Triple,
            SubscriptionTier::Ultra => HopMode::Quad,
        }
    }

    /// Convert a u8 tier value to SubscriptionTier (255 = free/unsubscribed).
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(SubscriptionTier::Basic),
            1 => Some(SubscriptionTier::Standard),
            2 => Some(SubscriptionTier::Premium),
            3 => Some(SubscriptionTier::Ultra),
            _ => None,
        }
    }

    /// Convert to u8 for on-chain representation.
    pub fn as_u8(&self) -> u8 {
        match self {
            SubscriptionTier::Basic => 0,
            SubscriptionTier::Standard => 1,
            SubscriptionTier::Premium => 2,
            SubscriptionTier::Ultra => 3,
        }
    }
}

/// Resolve the effective hop mode based on subscription tier.
///
/// - Free users (no tier) are forced to `HopMode::Direct`.
/// - Paid users get the requested hop mode clamped to their tier's maximum.
pub fn resolve_hop_mode(tier: Option<SubscriptionTier>, requested: HopMode) -> HopMode {
    match tier {
        None => HopMode::Direct,
        Some(t) => requested.clamp_to(t.max_hop_mode()),
    }
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
/// | Mode   | Relay hops | Path                                            |
/// |--------|-----------|--------------------------------------------------|
/// | Direct | 0         | client → exit (no relays, exit sees client IP)   |
/// | Single | 1         | client → gateway → exit                          |
/// | Double | 2         | client → gateway → relay → exit                  |
/// | Triple | 3         | client → gateway → relay → relay → exit          |
/// | Quad   | 4         | client → gateway → relay → relay → relay → exit  |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HopMode {
    /// 0 hops — client sends directly to exit. Free tier only.
    /// Exit sees client IP. Trades privacy for zero-cost access.
    Direct,
    /// 1 hop (gateway only) - fastest with a relay, exit sees gateway IP
    Single,
    /// 2 hops - basic privacy, no single node sees both client and exit
    Double,
    /// 3 hops - good privacy (default)
    Triple,
    /// 4 hops - maximum privacy
    Quad,
}

impl HopMode {
    /// Total number of relay hops in the path.
    /// Direct=0, Single=1, Double=2, Triple=3, Quad=4.
    pub fn min_relays(&self) -> u8 {
        match self {
            HopMode::Direct => 0,
            HopMode::Single => 1,
            HopMode::Double => 2,
            HopMode::Triple => 3,
            HopMode::Quad => 4,
        }
    }

    /// Extra relay hops beyond the gateway.
    /// Direct=0, Single=0, Double=1, Triple=2, Quad=3.
    pub fn extra_hops(&self) -> u8 {
        match self {
            HopMode::Direct => 0,
            _ => self.min_relays() - 1,
        }
    }

    /// Deprecated: use `min_relays()` instead.
    pub fn hop_count(&self) -> u8 {
        self.min_relays()
    }

    pub fn from_count(count: u8) -> Self {
        match count {
            0 => HopMode::Direct,
            1 => HopMode::Single,
            2 => HopMode::Double,
            3 => HopMode::Triple,
            _ => HopMode::Quad,
        }
    }

    /// Clamp this hop mode to at most the given maximum.
    pub fn clamp_to(self, max: HopMode) -> HopMode {
        if self > max { max } else { self }
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
/// The `pool_pubkey` identifies the subscription pool (ephemeral for
/// subscribers, persistent for free-tier users).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardReceipt {
    /// Per-hop unique shard identifier (includes relay_pubkey in derivation)
    pub shard_id: Id,
    /// Public key of the relay that forwarded the shard (anti-Sybil: binds receipt to sender)
    pub sender_pubkey: PublicKey,
    /// Public key of the receiving node (signs this receipt)
    pub receiver_pubkey: PublicKey,
    /// Ephemeral subscription pubkey (pool identity) or persistent pubkey for free-tier
    pub pool_pubkey: PublicKey,
    /// Actual payload bytes in the forwarded shard.
    /// Settlement weights rewards by bandwidth, not receipt count.
    pub payload_size: u32,
    /// Unix timestamp (seconds) when the shard was received
    pub timestamp: u64,
    /// Receiver's ed25519 signature over the receipt payload
    #[serde(with = "BigArray")]
    pub signature: Signature,
}

impl ForwardReceipt {
    /// Get the data that the receiver signs (140 bytes):
    /// shard_id(32) || sender_pubkey(32) || receiver_pubkey(32) || pool_pubkey(32) || payload_size_le(4) || timestamp_le(8)
    pub fn signable_data(
        shard_id: &Id,
        sender_pubkey: &PublicKey,
        receiver_pubkey: &PublicKey,
        pool_pubkey: &PublicKey,
        payload_size: u32,
        timestamp: u64,
    ) -> Vec<u8> {
        let mut data = Vec::with_capacity(32 + 32 + 32 + 32 + 4 + 8);
        data.extend_from_slice(shard_id);
        data.extend_from_slice(sender_pubkey);
        data.extend_from_slice(receiver_pubkey);
        data.extend_from_slice(pool_pubkey);
        data.extend_from_slice(&payload_size.to_le_bytes());
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
        assert_eq!(HopMode::Direct.min_relays(), 0);
        assert_eq!(HopMode::Single.min_relays(), 1);
        assert_eq!(HopMode::Double.min_relays(), 2);
        assert_eq!(HopMode::Triple.min_relays(), 3);
        assert_eq!(HopMode::Quad.min_relays(), 4);
    }

    #[test]
    fn test_hop_mode_extra_hops() {
        assert_eq!(HopMode::Direct.extra_hops(), 0);
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
        assert_eq!(HopMode::from_count(0), HopMode::Direct);
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
        for mode in [HopMode::Direct, HopMode::Single, HopMode::Double, HopMode::Triple, HopMode::Quad] {
            let count = mode.min_relays();
            assert_eq!(HopMode::from_count(count), mode);
        }
    }

    #[test]
    fn test_hop_mode_equality() {
        assert_eq!(HopMode::Direct, HopMode::Direct);
        assert_eq!(HopMode::Single, HopMode::Single);
        assert_ne!(HopMode::Direct, HopMode::Single);
        assert_ne!(HopMode::Single, HopMode::Double);
        assert_ne!(HopMode::Double, HopMode::Triple);
        assert_ne!(HopMode::Triple, HopMode::Quad);
    }

    #[test]
    fn test_hop_mode_ordering() {
        assert!(HopMode::Direct < HopMode::Single);
        assert!(HopMode::Single < HopMode::Double);
        assert!(HopMode::Double < HopMode::Triple);
        assert!(HopMode::Triple < HopMode::Quad);
    }

    #[test]
    fn test_hop_mode_clamp_to() {
        assert_eq!(HopMode::Quad.clamp_to(HopMode::Double), HopMode::Double);
        assert_eq!(HopMode::Single.clamp_to(HopMode::Quad), HopMode::Single);
        assert_eq!(HopMode::Direct.clamp_to(HopMode::Single), HopMode::Direct);
        assert_eq!(HopMode::Triple.clamp_to(HopMode::Triple), HopMode::Triple);
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
    fn test_subscription_tier_serialization() {
        for tier in [SubscriptionTier::Basic, SubscriptionTier::Standard, SubscriptionTier::Premium, SubscriptionTier::Ultra] {
            let json = serde_json::to_string(&tier).unwrap();
            let restored: SubscriptionTier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, restored);
        }
    }

    #[test]
    fn test_subscription_tier_max_hop_mode() {
        assert_eq!(SubscriptionTier::Basic.max_hop_mode(), HopMode::Single);
        assert_eq!(SubscriptionTier::Standard.max_hop_mode(), HopMode::Double);
        assert_eq!(SubscriptionTier::Premium.max_hop_mode(), HopMode::Triple);
        assert_eq!(SubscriptionTier::Ultra.max_hop_mode(), HopMode::Quad);
    }

    #[test]
    fn test_subscription_tier_from_u8() {
        assert_eq!(SubscriptionTier::from_u8(0), Some(SubscriptionTier::Basic));
        assert_eq!(SubscriptionTier::from_u8(1), Some(SubscriptionTier::Standard));
        assert_eq!(SubscriptionTier::from_u8(2), Some(SubscriptionTier::Premium));
        assert_eq!(SubscriptionTier::from_u8(3), Some(SubscriptionTier::Ultra));
        assert_eq!(SubscriptionTier::from_u8(4), None);
        assert_eq!(SubscriptionTier::from_u8(255), None);
    }

    #[test]
    fn test_subscription_tier_as_u8() {
        assert_eq!(SubscriptionTier::Basic.as_u8(), 0);
        assert_eq!(SubscriptionTier::Standard.as_u8(), 1);
        assert_eq!(SubscriptionTier::Premium.as_u8(), 2);
        assert_eq!(SubscriptionTier::Ultra.as_u8(), 3);
    }

    #[test]
    fn test_resolve_hop_mode() {
        // Free user → always Direct
        assert_eq!(resolve_hop_mode(None, HopMode::Quad), HopMode::Direct);
        assert_eq!(resolve_hop_mode(None, HopMode::Direct), HopMode::Direct);

        // Basic → clamped to Single
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Basic), HopMode::Quad), HopMode::Single);
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Basic), HopMode::Single), HopMode::Single);
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Basic), HopMode::Direct), HopMode::Direct);

        // Standard → clamped to Double
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Standard), HopMode::Quad), HopMode::Double);
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Standard), HopMode::Double), HopMode::Double);
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Standard), HopMode::Single), HopMode::Single);

        // Premium → up to Triple (3 hops max)
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Premium), HopMode::Quad), HopMode::Triple);
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Premium), HopMode::Triple), HopMode::Triple);

        // Ultra → up to Quad (4 hops max)
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Ultra), HopMode::Quad), HopMode::Quad);
        assert_eq!(resolve_hop_mode(Some(SubscriptionTier::Ultra), HopMode::Triple), HopMode::Triple);
    }

    // ==================== ForwardReceipt Tests ====================

    #[test]
    fn test_forward_receipt_signable_data() {
        let shard_id = [3u8; 32];
        let sender_pubkey = [5u8; 32];
        let receiver_pubkey = [2u8; 32];
        let pool_pubkey = [4u8; 32];
        let data = ForwardReceipt::signable_data(&shard_id, &sender_pubkey, &receiver_pubkey, &pool_pubkey, 1024, 1000);

        // 32 (shard_id) + 32 (sender_pubkey) + 32 (receiver_pubkey) + 32 (pool_pubkey) + 4 (payload_size) + 8 (timestamp) = 140
        assert_eq!(data.len(), 140);
        assert_eq!(&data[0..32], &shard_id);
        assert_eq!(&data[32..64], &sender_pubkey);
        assert_eq!(&data[64..96], &receiver_pubkey);
        assert_eq!(&data[96..128], &pool_pubkey);
        assert_eq!(&data[128..132], &1024u32.to_le_bytes());
        assert_eq!(&data[132..140], &1000u64.to_le_bytes());
    }

    #[test]
    fn test_forward_receipt_signable_data_different_inputs() {
        let pool_pubkey = [5u8; 32];
        let sender = [9u8; 32];
        let data1 = ForwardReceipt::signable_data(&[10u8; 32], &sender, &[2u8; 32], &pool_pubkey, 1024, 100);
        let data2 = ForwardReceipt::signable_data(&[11u8; 32], &sender, &[2u8; 32], &pool_pubkey, 1024, 100);
        // Different shard_id should produce different data
        assert_ne!(data1, data2);
    }

    #[test]
    fn test_forward_receipt_same_relay_different_shards() {
        let pool_pubkey = [5u8; 32];
        let sender = [9u8; 32];
        let data1 = ForwardReceipt::signable_data(&[10u8; 32], &sender, &[2u8; 32], &pool_pubkey, 1024, 100);
        let data2 = ForwardReceipt::signable_data(&[20u8; 32], &sender, &[2u8; 32], &pool_pubkey, 1024, 100);
        assert_ne!(data1, data2);
    }

    #[test]
    fn test_forward_receipt_different_pool_pubkeys() {
        let sender = [9u8; 32];
        let data1 = ForwardReceipt::signable_data(&[10u8; 32], &sender, &[2u8; 32], &[5u8; 32], 1024, 100);
        let data2 = ForwardReceipt::signable_data(&[10u8; 32], &sender, &[2u8; 32], &[6u8; 32], 1024, 100);
        assert_ne!(data1, data2);
    }

    #[test]
    fn test_forward_receipt_different_senders() {
        let pool_pubkey = [5u8; 32];
        let data1 = ForwardReceipt::signable_data(&[10u8; 32], &[9u8; 32], &[2u8; 32], &pool_pubkey, 1024, 100);
        let data2 = ForwardReceipt::signable_data(&[10u8; 32], &[8u8; 32], &[2u8; 32], &pool_pubkey, 1024, 100);
        assert_ne!(data1, data2, "Different senders should produce different signable data");
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

    // ==================== Capabilities Tests ====================

    #[test]
    fn test_capabilities_default() {
        assert_eq!(Capabilities::default(), Capabilities::CLIENT);
    }

    #[test]
    fn test_capabilities_helpers() {
        let client = Capabilities::CLIENT;
        assert!(client.is_client());
        assert!(!client.is_relay());
        assert!(!client.is_exit());
        assert!(!client.is_aggregator());
        assert!(!client.is_service_node());

        let relay = Capabilities::RELAY;
        assert!(!relay.is_client());
        assert!(relay.is_relay());
        assert!(relay.is_service_node());

        let exit = Capabilities::EXIT;
        assert!(exit.is_exit());
        assert!(exit.is_service_node());

        let agg = Capabilities::AGGREGATOR;
        assert!(agg.is_aggregator());
        assert!(agg.is_service_node());
    }

    #[test]
    fn test_capabilities_composition() {
        let full = Capabilities::RELAY | Capabilities::EXIT;
        assert!(full.is_relay());
        assert!(full.is_exit());
        assert!(!full.is_client());
        assert!(full.is_service_node());

        let both = Capabilities::CLIENT | Capabilities::RELAY;
        assert!(both.is_client());
        assert!(both.is_relay());
        assert!(both.is_service_node());

        let all = Capabilities::CLIENT | Capabilities::RELAY | Capabilities::EXIT | Capabilities::AGGREGATOR;
        assert!(all.is_client());
        assert!(all.is_relay());
        assert!(all.is_exit());
        assert!(all.is_aggregator());
        assert!(all.is_service_node());
    }

    #[test]
    fn test_capabilities_empty() {
        let empty = Capabilities::empty();
        assert!(!empty.is_client());
        assert!(!empty.is_relay());
        assert!(!empty.is_exit());
        assert!(!empty.is_aggregator());
        assert!(!empty.is_service_node());
    }
}

//! Onion-routed shard
//!
//! A shard is a fragment of an erasure-coded, onion-encrypted request or response.
//! No plaintext routing metadata is visible — each relay peels one onion layer
//! from the header to learn the next hop.

use serde::{Deserialize, Serialize};

/// A shard carrying an onion-encrypted payload fragment
///
/// All routing metadata is inside the encrypted `header` (onion layers).
/// The `routing_tag` lets the exit group shards by assembly_id for reconstruction.
/// Shard/chunk metadata is encrypted inside the routing_tag (only exit/client needs it).
///
/// `total_hops` and `hops_remaining` are public fields for tier enforcement:
/// - `total_hops`: total relay hops in the path (set by client, never changes)
/// - `hops_remaining`: decremented by each relay before forwarding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shard {
    /// Ephemeral X25519 pubkey for ECDH with the current hop
    pub ephemeral_pubkey: [u8; 32],
    /// Encrypted onion routing layers (each relay peels one)
    pub header: Vec<u8>,
    /// Erasure-coded piece of exit-encrypted data
    pub payload: Vec<u8>,
    /// Exit-encrypted routing tag containing assembly_id + shard/chunk metadata
    /// Format: [ephemeral_pubkey: 32][nonce: 12][encrypted(RoutingTag)]
    pub routing_tag: Vec<u8>,
    /// Total relay hops in the path (set by client, never modified in transit).
    /// Used by relays for tier enforcement: `total_hops <= max_for_tier(pool_pubkey)`.
    #[serde(default)]
    pub total_hops: u8,
    /// Remaining relay hops. Decremented by each relay before forwarding.
    /// When 0, no honest relay will process the shard further.
    #[serde(default)]
    pub hops_remaining: u8,
}

impl Shard {
    /// Create a new onion-routed shard
    pub fn new(
        ephemeral_pubkey: [u8; 32],
        header: Vec<u8>,
        payload: Vec<u8>,
        routing_tag: Vec<u8>,
        total_hops: u8,
        hops_remaining: u8,
    ) -> Self {
        Self {
            ephemeral_pubkey,
            header,
            payload,
            routing_tag,
            total_hops,
            hops_remaining,
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

/// Wire format header magic bytes (new format: "TCON" = CraftNet ONion)
pub const SHARD_MAGIC: [u8; 4] = [0x54, 0x43, 0x4F, 0x4E]; // "TCON"

/// Current wire format version
pub const SHARD_VERSION: u8 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_shard() {
        let shard = Shard::new(
            [1u8; 32],          // ephemeral_pubkey
            vec![2, 3, 4],      // header
            vec![5, 6, 7, 8],   // payload
            vec![9; 98],        // routing_tag (larger now with metadata)
            3,                   // total_hops
            3,                   // hops_remaining
        );

        assert_eq!(shard.ephemeral_pubkey, [1u8; 32]);
        assert_eq!(shard.header, vec![2, 3, 4]);
        assert_eq!(shard.payload, vec![5, 6, 7, 8]);
        assert_eq!(shard.routing_tag.len(), 98);
        assert_eq!(shard.total_hops, 3);
        assert_eq!(shard.hops_remaining, 3);
    }

    #[test]
    fn test_shard_has_no_plaintext_metadata() {
        // Verify that Shard struct has no shard_index, total_shards, chunk_index, total_chunks
        let shard = Shard::new(
            [0u8; 32],
            vec![],
            vec![1, 2, 3],
            vec![0; 98],
            0, 0,
        );
        // These fields no longer exist on Shard — all metadata is in encrypted routing_tag
        let json = serde_json::to_string(&shard).unwrap();
        assert!(!json.contains("shard_index"));
        assert!(!json.contains("total_shards"));
        assert!(!json.contains("chunk_index"));
        assert!(!json.contains("total_chunks"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let shard = Shard::new(
            [1u8; 32],
            vec![2, 3, 4],
            vec![5, 6, 7, 8],
            vec![9; 98],
            2, 2,
        );

        let bytes = shard.to_bytes().unwrap();
        let restored = Shard::from_bytes(&bytes).unwrap();

        assert_eq!(restored.ephemeral_pubkey, shard.ephemeral_pubkey);
        assert_eq!(restored.header, shard.header);
        assert_eq!(restored.payload, shard.payload);
        assert_eq!(restored.routing_tag, shard.routing_tag);
    }

    #[test]
    fn test_deserialization_invalid_data() {
        let result = Shard::from_bytes(&[0xFF, 0xFE, 0xFD]);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialization_empty() {
        let result = Shard::from_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_header_direct_mode() {
        let shard = Shard::new(
            [0u8; 32],
            vec![],         // empty header = direct mode (0 hops)
            vec![1, 2, 3],
            vec![0; 98],
            0, 0,
        );

        assert!(shard.header.is_empty());
        let bytes = shard.to_bytes().unwrap();
        let restored = Shard::from_bytes(&bytes).unwrap();
        assert!(restored.header.is_empty());
    }

    #[test]
    fn test_magic_bytes() {
        assert_eq!(SHARD_MAGIC, [0x54, 0x43, 0x4F, 0x4E]);
        assert_eq!(SHARD_MAGIC[0], b'T');
        assert_eq!(SHARD_MAGIC[1], b'C');
        assert_eq!(SHARD_MAGIC[2], b'O');
        assert_eq!(SHARD_MAGIC[3], b'N');
    }

    #[test]
    fn test_shard_version() {
        assert_eq!(SHARD_VERSION, 2);
    }

    #[test]
    fn test_large_payload() {
        let large_payload = vec![0xAB; 1024 * 1024];
        let shard = Shard::new(
            [0u8; 32],
            vec![1; 540],       // 3-hop header
            large_payload.clone(),
            vec![0; 98],
            3, 3,
        );

        assert_eq!(shard.payload.len(), 1024 * 1024);
        let bytes = shard.to_bytes().unwrap();
        let restored = Shard::from_bytes(&bytes).unwrap();
        assert_eq!(restored.payload, large_payload);
    }

    #[test]
    fn test_empty_payload() {
        let shard = Shard::new(
            [0u8; 32],
            vec![],
            vec![],
            vec![],
            0, 0,
        );

        assert!(shard.payload.is_empty());
        let bytes = shard.to_bytes().unwrap();
        let restored = Shard::from_bytes(&bytes).unwrap();
        assert!(restored.payload.is_empty());
    }
}

//! Onion routing types for anonymous shard delivery
//!
//! These types define the encrypted layer structure for multi-hop onion routing.
//! Each relay peels one layer from the shard header to learn the next hop.
//! No plaintext routing metadata is visible to intermediate relays.

use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};

use crate::types::{Id, PublicKey};

/// Decrypted onion layer revealed when a relay peels one encryption layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnionLayer {
    /// PeerId of the next hop (relay or exit)
    pub next_peer_id: Vec<u8>,
    /// Ephemeral pubkey for next hop's ECDH (replaces shard.ephemeral_pubkey)
    pub next_ephemeral_pubkey: [u8; 32],
    /// Settlement data for this relay's ForwardReceipt
    pub settlement: OnionSettlement,
    /// Remaining encrypted header blob for the next relay
    pub remaining_header: Vec<u8>,
    /// Whether this is the last relay before exit/client
    pub is_terminal: bool,
    /// Present when this relay should act as gateway (deliver to client via tunnel_id)
    pub tunnel_id: Option<Id>,
}

/// Per-hop settlement data encrypted inside each onion layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnionSettlement {
    /// Per-hop unique blind token: SHA256(user_proof || shard_id || relay_pubkey)
    /// Different for each relay on the path — prevents cross-relay correlation.
    pub blind_token: Id,
    /// Per-hop unique shard identifier: SHA256(request_id || "shard" || chunk_index || shard_index || relay_pubkey)
    pub shard_id: Id,
    /// Actual payload bytes (for bandwidth-weighted settlement)
    pub payload_size: u32,
    /// Subscription epoch (anti-replay)
    pub epoch: u64,
    /// Ephemeral subscription pubkey identifying the user's pool PDA.
    /// [0u8; 32] for free-tier (no subscription).
    #[serde(default)]
    pub pool_pubkey: PublicKey,
}

/// Shard type indicator (moved here from shard.rs — only visible inside encrypted payload)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShardType {
    Request,
    Response,
}

/// Full request/response payload decrypted by the exit node
///
/// After erasure-code reconstruction and decryption, the exit sees this.
/// Contains all routing metadata that was previously on the Shard in plaintext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitPayload {
    /// Real request_id (only exit sees this)
    pub request_id: Id,
    /// User's signing pubkey (for settlement)
    pub user_pubkey: PublicKey,
    /// User proof copied into response shards
    pub user_proof: Id,
    /// Lease set for response routing back to client
    pub lease_set: crate::lease_set::LeaseSet,
    /// Total relay hops for response path
    pub total_hops: u8,
    /// Request or Response
    pub shard_type: ShardType,
    /// 0x00 HTTP, 0x01 tunnel
    pub mode: u8,
    /// HTTP request bytes or tunnel metadata + TCP bytes
    pub data: Vec<u8>,
    /// Client's X25519 encryption pubkey for response encryption.
    /// The exit uses this key (not user_pubkey) to encrypt response data,
    /// because ECDH requires X25519 keys while user_pubkey is Ed25519.
    #[serde(default)]
    pub response_enc_pubkey: PublicKey,
}

/// Routing tag data (encrypted for exit, per-shard)
///
/// Contains assembly grouping ID plus shard/chunk metadata that was
/// previously plaintext on Shard. Now only the exit/client can see this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingTag {
    /// Assembly ID for grouping shards at the exit
    pub assembly_id: Id,
    /// Shard index within this chunk's erasure coding set
    pub shard_index: u8,
    /// Total shards in this chunk's erasure coding set
    pub total_shards: u8,
    /// Which chunk this shard belongs to (0-indexed)
    pub chunk_index: u16,
    /// Total number of chunks in this request/response
    pub total_chunks: u16,
}

/// Compute per-hop blind token: SHA256(user_proof || shard_id || relay_pubkey)
///
/// Each relay on a path sees a unique blind_token derived from the same
/// user_proof. Prevents colluding relays from correlating settlement data.
pub fn compute_blind_token(user_proof: &Id, shard_id: &Id, relay_pubkey: &PublicKey) -> Id {
    let mut hasher = Sha256::new();
    hasher.update(user_proof);
    hasher.update(shard_id);
    hasher.update(relay_pubkey);
    let result = hasher.finalize();
    let mut token = [0u8; 32];
    token.copy_from_slice(&result);
    token
}

impl OnionLayer {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

impl OnionSettlement {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

impl ExitPayload {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

impl RoutingTag {
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
    use crate::lease_set::LeaseSet;

    #[test]
    fn test_onion_layer_serde() {
        let layer = OnionLayer {
            next_peer_id: vec![1, 2, 3, 4],
            next_ephemeral_pubkey: [5u8; 32],
            settlement: OnionSettlement {
                blind_token: [6u8; 32],
                shard_id: [7u8; 32],
                payload_size: 1024,
                epoch: 42,
                pool_pubkey: [0u8; 32],
            },
            remaining_header: vec![8, 9, 10],
            is_terminal: false,
            tunnel_id: None,
        };

        let bytes = layer.to_bytes().unwrap();
        let restored = OnionLayer::from_bytes(&bytes).unwrap();
        assert_eq!(restored.next_peer_id, layer.next_peer_id);
        assert_eq!(restored.next_ephemeral_pubkey, layer.next_ephemeral_pubkey);
        assert_eq!(restored.settlement.payload_size, 1024);
        assert!(!restored.is_terminal);
        assert!(restored.tunnel_id.is_none());
    }

    #[test]
    fn test_onion_layer_with_tunnel_id() {
        let layer = OnionLayer {
            next_peer_id: vec![],
            next_ephemeral_pubkey: [0u8; 32],
            settlement: OnionSettlement {
                blind_token: [0u8; 32],
                shard_id: [0u8; 32],
                payload_size: 0,
                epoch: 0,
                pool_pubkey: [0u8; 32],
            },
            remaining_header: vec![],
            is_terminal: true,
            tunnel_id: Some([99u8; 32]),
        };

        let bytes = layer.to_bytes().unwrap();
        let restored = OnionLayer::from_bytes(&bytes).unwrap();
        assert!(restored.is_terminal);
        assert_eq!(restored.tunnel_id, Some([99u8; 32]));
    }

    #[test]
    fn test_exit_payload_serde() {
        let payload = ExitPayload {
            request_id: [1u8; 32],
            user_pubkey: [2u8; 32],
            user_proof: [3u8; 32],
            lease_set: LeaseSet {
                session_id: [4u8; 32],
                leases: vec![],
            },
            total_hops: 2,
            shard_type: ShardType::Request,
            mode: 0x01,
            data: vec![5, 6, 7],
            response_enc_pubkey: [0u8; 32],
        };

        let bytes = payload.to_bytes().unwrap();
        let restored = ExitPayload::from_bytes(&bytes).unwrap();
        assert_eq!(restored.request_id, [1u8; 32]);
        assert_eq!(restored.user_pubkey, [2u8; 32]);
        assert_eq!(restored.total_hops, 2);
        assert_eq!(restored.shard_type, ShardType::Request);
        assert_eq!(restored.mode, 0x01);
        assert_eq!(restored.data, vec![5, 6, 7]);
    }

    #[test]
    fn test_shard_type_equality() {
        assert_eq!(ShardType::Request, ShardType::Request);
        assert_eq!(ShardType::Response, ShardType::Response);
        assert_ne!(ShardType::Request, ShardType::Response);
    }

    #[test]
    fn test_routing_tag_serde() {
        let tag = RoutingTag {
            assembly_id: [42u8; 32],
            shard_index: 2,
            total_shards: 5,
            chunk_index: 1,
            total_chunks: 3,
        };
        let bytes = tag.to_bytes().unwrap();
        let restored = RoutingTag::from_bytes(&bytes).unwrap();
        assert_eq!(restored.assembly_id, [42u8; 32]);
        assert_eq!(restored.shard_index, 2);
        assert_eq!(restored.total_shards, 5);
        assert_eq!(restored.chunk_index, 1);
        assert_eq!(restored.total_chunks, 3);
    }

    #[test]
    fn test_compute_blind_token_deterministic() {
        let user_proof = [1u8; 32];
        let shard_id = [2u8; 32];
        let relay_pubkey = [3u8; 32];

        let t1 = compute_blind_token(&user_proof, &shard_id, &relay_pubkey);
        let t2 = compute_blind_token(&user_proof, &shard_id, &relay_pubkey);
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_compute_blind_token_unique_per_relay() {
        let user_proof = [1u8; 32];
        let shard_id = [2u8; 32];
        let relay_a = [3u8; 32];
        let relay_b = [4u8; 32];

        let t_a = compute_blind_token(&user_proof, &shard_id, &relay_a);
        let t_b = compute_blind_token(&user_proof, &shard_id, &relay_b);
        assert_ne!(t_a, t_b, "Same user_proof and shard_id should yield different tokens for different relays");
    }

    #[test]
    fn test_compute_blind_token_unique_per_shard_id() {
        let user_proof = [1u8; 32];
        let shard_id_a = [2u8; 32];
        let shard_id_b = [3u8; 32];
        let relay = [4u8; 32];

        let t_a = compute_blind_token(&user_proof, &shard_id_a, &relay);
        let t_b = compute_blind_token(&user_proof, &shard_id_b, &relay);
        assert_ne!(t_a, t_b);
    }
}

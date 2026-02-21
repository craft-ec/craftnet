//! Shared shard building pipeline (socket/HTTP parity)
//!
//! Both HTTP mode (`request.rs`) and tunnel mode (`tunnel.rs`) use the same
//! pipeline: encrypt → frame → erasure code → onion wrap. This module provides
//! the shared implementation.

use sha2::{Sha256, Digest};

use craftnet_core::{
    Shard, Id, PublicKey, ExitPayload, ShardType, OnionSettlement,
    lease_set::LeaseSet,
};
use craftec_crypto::{SigningKeypair};
use craftnet_core::onion_crypto::{build_onion_header, encrypt_exit_payload, encrypt_routing_tag};
use craftnet_erasure::TOTAL_SHARDS;
use craftnet_erasure::chunker::chunk_and_encode;

use crate::path::{OnionPath, PathHop, random_id};
use crate::{ClientError, Result};

/// Build onion-routed shards from mode-specific payload data.
///
/// Shared pipeline for both HTTP and tunnel modes:
/// 1. Create ExitPayload with given mode + data
/// 2. Encrypt for exit node
/// 3. Frame with 4-byte LE length prefix (both modes)
/// 4. Chunk and erasure code
/// 5. Wrap each shard in onion header with per-hop settlement
///
/// # Arguments
/// * `mode` - `PAYLOAD_MODE_HTTP` (0x00) or `PAYLOAD_MODE_TUNNEL` (0x01)
/// * `payload_data` - Mode-specific data (serialized HTTP request or tunnel metadata+tcp)
/// * `response_enc_pubkey` - Client's X25519 key for response encryption
/// * `keypair` - User's signing keypair
/// * `exit` - Exit node hop info
/// * `paths` - Per-shard onion paths (one per shard, or round-robin)
/// * `lease_set` - LeaseSet for response routing
/// * `pool_pubkey` - Ephemeral subscription key or persistent free-tier key
pub fn build_onion_shards(
    mode: u8,
    payload_data: Vec<u8>,
    response_enc_pubkey: [u8; 32],
    keypair: &SigningKeypair,
    exit: &PathHop,
    paths: &[OnionPath],
    lease_set: &LeaseSet,
    pool_pubkey: PublicKey,
) -> Result<(Id, Vec<Shard>)> {
    let request_id = random_id();
    let assembly_id = random_id();
    let user_pubkey = keypair.public_key_bytes();

    // Build ExitPayload
    let exit_payload = ExitPayload {
        request_id,
        user_pubkey,
        lease_set: lease_set.clone(),
        total_hops: paths.first().map(|p| p.hops.len() as u8).unwrap_or(0),
        shard_type: ShardType::Request,
        mode,
        data: payload_data,
        response_enc_pubkey,
    };

    // Encrypt for exit
    let encrypted = encrypt_exit_payload(
        &exit.encryption_pubkey,
        &exit_payload,
    ).map_err(|e| ClientError::CryptoError(e.to_string()))?;

    // Frame: prepend original length (4-byte LE u32) so exit can strip erasure padding.
    // Both HTTP and tunnel modes get this framing — fixes tunnel mode bug where
    // it previously skipped the length prefix.
    let original_len = encrypted.len() as u32;
    let mut framed = Vec::with_capacity(4 + encrypted.len());
    framed.extend_from_slice(&original_len.to_le_bytes());
    framed.extend_from_slice(&encrypted);

    // Chunk and erasure code
    let chunks = chunk_and_encode(&framed)
        .map_err(|e| ClientError::ErasureError(e.to_string()))?;

    let total_chunks = chunks.len() as u16;
    let mut shards = Vec::with_capacity(chunks.len() * TOTAL_SHARDS);

    for (chunk_index, shard_payloads) in chunks {
        let total_shards_in_chunk = shard_payloads.len() as u8;

        for (i, payload) in shard_payloads.into_iter().enumerate() {
            let path = if paths.is_empty() {
                &OnionPath { hops: vec![], exit: exit.clone() }
            } else {
                &paths[i % paths.len()]
            };

            // Build per-hop settlement data with unique shard_id per relay
            let settlement: Vec<OnionSettlement> = path.hops.iter().map(|hop| {
                let shard_id = generate_shard_id(&request_id, chunk_index, i as u8, &hop.signing_pubkey);
                OnionSettlement {
                    shard_id,
                    payload_size: payload.len() as u32,
                    pool_pubkey,
                }
            }).collect();

            // Build onion header
            let hops_for_header: Vec<(&[u8], &[u8; 32])> = path.hops.iter()
                .map(|h| (h.peer_id.as_slice(), &h.encryption_pubkey))
                .collect();

            let (header, ephemeral) = build_onion_header(
                &hops_for_header,
                (exit.peer_id.as_slice(), &exit.encryption_pubkey),
                &settlement,
                None,
            ).map_err(|e| ClientError::CryptoError(e.to_string()))?;

            // Encrypt routing tag with shard/chunk metadata
            let routing_tag = encrypt_routing_tag(
                &exit.encryption_pubkey,
                &assembly_id,
                i as u8,
                total_shards_in_chunk,
                chunk_index,
                total_chunks,
                &pool_pubkey,
            ).map_err(|e| ClientError::CryptoError(e.to_string()))?;

            let total_hops = path.hops.len() as u8;
            shards.push(Shard::new(
                ephemeral,
                header,
                payload,
                routing_tag,
                total_hops,
                total_hops, // hops_remaining starts equal to total_hops
            ));
        }
    }

    Ok((request_id, shards))
}

/// Generate a per-hop unique shard ID: SHA256(request_id || "shard" || chunk_index || shard_index || relay_pubkey)
pub fn generate_shard_id(request_id: &Id, chunk_index: u16, shard_index: u8, relay_pubkey: &PublicKey) -> Id {
    let mut hasher = Sha256::new();
    hasher.update(request_id);
    hasher.update(b"shard");
    hasher.update(chunk_index.to_be_bytes());
    hasher.update([shard_index]);
    hasher.update(relay_pubkey);
    let result = hasher.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(&result);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_id_deterministic() {
        let request_id: Id = [42u8; 32];
        let relay = [1u8; 32];
        let a = generate_shard_id(&request_id, 0, 0, &relay);
        let b = generate_shard_id(&request_id, 0, 0, &relay);
        assert_eq!(a, b);
    }

    #[test]
    fn test_shard_id_unique_per_index() {
        let request_id: Id = [42u8; 32];
        let relay = [1u8; 32];
        let id0 = generate_shard_id(&request_id, 0, 0, &relay);
        let id1 = generate_shard_id(&request_id, 0, 1, &relay);
        let id2 = generate_shard_id(&request_id, 1, 0, &relay);
        assert_ne!(id0, id1);
        assert_ne!(id0, id2);
    }

    #[test]
    fn test_shard_id_unique_per_relay() {
        let request_id: Id = [42u8; 32];
        let relay_a = [1u8; 32];
        let relay_b = [2u8; 32];
        let id_a = generate_shard_id(&request_id, 0, 0, &relay_a);
        let id_b = generate_shard_id(&request_id, 0, 0, &relay_b);
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn test_build_onion_shards_direct_http() {
        let keypair = SigningKeypair::generate();
        let enc_keypair = craftec_crypto::EncryptionKeypair::generate();

        let exit = PathHop {
            peer_id: b"exit_peer".to_vec(),
            signing_pubkey: [2u8; 32],
            encryption_pubkey: enc_keypair.public_key_bytes(),
        };

        let lease_set = LeaseSet {
            session_id: [0u8; 32],
            leases: vec![],
        };

        let (request_id, shards) = build_onion_shards(
            0x00, // HTTP mode
            b"GET\nhttps://example.com\n0\n0\n".to_vec(),
            [0u8; 32],
            &keypair,
            &exit,
            &[], // direct mode
            &lease_set,
            [0u8; 32],
        ).unwrap();

        assert_eq!(shards.len(), TOTAL_SHARDS);
        assert_ne!(request_id, [0u8; 32]);

        for shard in &shards {
            assert!(shard.header.is_empty());
            assert!(!shard.routing_tag.is_empty());
            assert_eq!(shard.total_hops, 0);
            assert_eq!(shard.hops_remaining, 0);
        }
    }

    #[test]
    fn test_build_onion_shards_direct_tunnel() {
        let keypair = SigningKeypair::generate();
        let enc_keypair = craftec_crypto::EncryptionKeypair::generate();

        let exit = PathHop {
            peer_id: b"exit_peer".to_vec(),
            signing_pubkey: [2u8; 32],
            encryption_pubkey: enc_keypair.public_key_bytes(),
        };

        let lease_set = LeaseSet {
            session_id: [0u8; 32],
            leases: vec![],
        };

        let (request_id, shards) = build_onion_shards(
            0x01, // Tunnel mode
            vec![0; 64], // dummy tunnel payload
            [99u8; 32], // response enc pubkey
            &keypair,
            &exit,
            &[], // direct mode
            &lease_set,
            [0u8; 32],
        ).unwrap();

        assert_eq!(shards.len(), TOTAL_SHARDS);
        assert_ne!(request_id, [0u8; 32]);

        for shard in &shards {
            assert!(shard.header.is_empty());
            assert_eq!(shard.total_hops, 0);
            assert_eq!(shard.hops_remaining, 0);
        }
    }
}

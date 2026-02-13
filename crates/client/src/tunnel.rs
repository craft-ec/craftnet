//! Tunnel-mode shard builder (onion-routed)
//!
//! Constructs onion-routed shards from raw TCP bytes for SOCKS5 tunnel mode.
//! The tunnel metadata + TCP data are placed inside ExitPayload.data with
//! mode = PAYLOAD_MODE_TUNNEL (0x01).

use sha2::{Sha256, Digest};

use tunnelcraft_core::{
    Shard, Id, PublicKey, ExitPayload, ShardType, OnionSettlement,
    TunnelMetadata, PAYLOAD_MODE_TUNNEL,
    lease_set::LeaseSet,
};
use tunnelcraft_crypto::{
    SigningKeypair, build_onion_header, encrypt_exit_payload, encrypt_routing_tag,
};
use tunnelcraft_erasure::TOTAL_SHARDS;
use tunnelcraft_erasure::chunker::chunk_and_encode;

use crate::path::{OnionPath, PathHop, random_id};
use crate::{ClientError, Result};

/// Build tunnel-mode onion-routed shards from raw TCP bytes.
///
/// Returns `(request_id, Vec<Shard>)`.
pub fn build_tunnel_shards(
    metadata: &TunnelMetadata,
    tcp_data: &[u8],
    keypair: &SigningKeypair,
    exit: &PathHop,
    paths: &[OnionPath],
    lease_set: &LeaseSet,
    pool_pubkey: PublicKey,
) -> Result<(Id, Vec<Shard>)> {
    let request_id = random_id();
    let assembly_id = random_id();
    let user_pubkey = keypair.public_key_bytes();

    // Build payload: [metadata_len: u32 BE] [metadata bincode] [tcp_data]
    // (mode byte is NOT in data — it's the ExitPayload.mode field)
    let metadata_bytes = metadata.to_bytes();
    let metadata_len = metadata_bytes.len() as u32;

    let mut payload_data = Vec::with_capacity(4 + metadata_bytes.len() + tcp_data.len());
    payload_data.extend_from_slice(&metadata_len.to_be_bytes());
    payload_data.extend_from_slice(&metadata_bytes);
    payload_data.extend_from_slice(tcp_data);

    // Build ExitPayload
    let exit_payload = ExitPayload {
        request_id,
        user_pubkey,
        lease_set: lease_set.clone(),
        total_hops: paths.first().map(|p| p.hops.len() as u8).unwrap_or(0),
        shard_type: ShardType::Request,
        mode: PAYLOAD_MODE_TUNNEL,
        data: payload_data,
        response_enc_pubkey: [0u8; 32],
    };

    // Encrypt for exit
    let encrypted = encrypt_exit_payload(
        &exit.encryption_pubkey,
        &exit_payload,
    ).map_err(|e| ClientError::CryptoError(e.to_string()))?;

    // Chunk and erasure code
    let chunks = chunk_and_encode(&encrypted)
        .map_err(|e| ClientError::ErasureError(e.to_string()))?;

    let total_chunks = chunks.len() as u16;

    let mut shards = Vec::with_capacity(chunks.len() * TOTAL_SHARDS);

    for (chunk_index, shard_payloads) in chunks {
        let total_shards_in_chunk = shard_payloads.len() as u8;

        for (i, shard_payload) in shard_payloads.into_iter().enumerate() {
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
                    payload_size: shard_payload.len() as u32,
                    pool_pubkey,
                }
            }).collect();

            let hops_for_header: Vec<(&[u8], &[u8; 32])> = path.hops.iter()
                .map(|h| (h.peer_id.as_slice(), &h.encryption_pubkey))
                .collect();

            let (header, ephemeral) = build_onion_header(
                &hops_for_header,
                (exit.peer_id.as_slice(), &exit.encryption_pubkey),
                &settlement,
                None,
            ).map_err(|e| ClientError::CryptoError(e.to_string()))?;

            let routing_tag = encrypt_routing_tag(
                &exit.encryption_pubkey,
                &assembly_id,
                i as u8,
                total_shards_in_chunk,
                chunk_index,
                total_chunks,
            ).map_err(|e| ClientError::CryptoError(e.to_string()))?;

            shards.push(Shard::new(
                ephemeral,
                header,
                shard_payload,
                routing_tag,
            ));
        }
    }

    Ok((request_id, shards))
}

/// Generate a per-hop unique shard ID: SHA256(request_id || "shard" || chunk_index || shard_index || relay_pubkey)
fn generate_shard_id(request_id: &Id, chunk_index: u16, shard_index: u8, relay_pubkey: &PublicKey) -> Id {
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
    fn test_build_tunnel_shards_direct() {
        let keypair = SigningKeypair::generate();
        let enc_keypair = tunnelcraft_crypto::EncryptionKeypair::generate();

        let exit = PathHop {
            peer_id: b"exit_peer".to_vec(),
            signing_pubkey: [2u8; 32],
            encryption_pubkey: enc_keypair.public_key_bytes(),
        };

        let metadata = TunnelMetadata {
            host: "example.com".to_string(),
            port: 443,
            session_id: [42u8; 32],
            is_close: false,
        };

        let lease_set = LeaseSet {
            session_id: [0u8; 32],
            leases: vec![],
        };

        let tcp_data = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let (request_id, shards) = build_tunnel_shards(
            &metadata,
            tcp_data,
            &keypair,
            &exit,
            &[], // direct mode
            &lease_set,
            [0u8; 32],
        ).unwrap();

        assert!(!shards.is_empty());
        assert_ne!(request_id, [0u8; 32]);

        // Direct mode: all shards have empty headers
        for shard in &shards {
            assert!(shard.header.is_empty());
        }
    }
}

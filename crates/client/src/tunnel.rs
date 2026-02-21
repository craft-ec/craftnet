//! Tunnel-mode shard builder (onion-routed)
//!
//! Constructs onion-routed shards from raw TCP bytes for SOCKS5 tunnel mode.
//! Delegates to the shared shard builder for the encrypt → frame → erasure → onion pipeline.

use craftnet_core::{
    Shard, Id, PublicKey,
    TunnelMetadata, PAYLOAD_MODE_TUNNEL,
    lease_set::LeaseSet,
};
use craftec_crypto::SigningKeypair;

use crate::path::{OnionPath, PathHop};
use crate::shard_builder::build_onion_shards;
use crate::Result;

/// Build tunnel-mode onion-routed shards from raw TCP bytes.
///
/// # Arguments
/// * `metadata` - Tunnel session metadata (host, port, session_id, is_close)
/// * `tcp_data` - Raw TCP bytes to pipe to destination
/// * `keypair` - User's signing keypair
/// * `exit` - Exit node hop info
/// * `paths` - Per-shard onion paths
/// * `lease_set` - LeaseSet for response routing
/// * `response_enc_pubkey` - Client's X25519 key for response encryption
/// * `pool_pubkey` - Ephemeral subscription key or persistent free-tier key
///
/// Returns `(request_id, Vec<Shard>)`.
pub fn build_tunnel_shards(
    metadata: &TunnelMetadata,
    tcp_data: &[u8],
    keypair: &SigningKeypair,
    exit: &PathHop,
    paths: &[OnionPath],
    lease_set: &LeaseSet,
    response_enc_pubkey: [u8; 32],
    pool_pubkey: PublicKey,
) -> Result<(Id, Vec<Shard>)> {
    // Build payload: [metadata_len: u32 BE] [metadata bincode] [tcp_data]
    // (mode byte is NOT in data — it's the ExitPayload.mode field)
    let metadata_bytes = metadata.to_bytes();
    let metadata_len = metadata_bytes.len() as u32;

    let mut payload_data = Vec::with_capacity(4 + metadata_bytes.len() + tcp_data.len());
    payload_data.extend_from_slice(&metadata_len.to_be_bytes());
    payload_data.extend_from_slice(&metadata_bytes);
    payload_data.extend_from_slice(tcp_data);

    build_onion_shards(
        PAYLOAD_MODE_TUNNEL,
        payload_data,
        response_enc_pubkey,
        keypair,
        exit,
        paths,
        lease_set,
        pool_pubkey,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use craftnet_erasure::TOTAL_SHARDS;

    #[test]
    fn test_build_tunnel_shards_direct() {
        let keypair = SigningKeypair::generate();
        let enc_keypair = craftec_crypto::EncryptionKeypair::generate();

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
            [0u8; 32], // response_enc_pubkey
            [0u8; 32], // pool_pubkey
        ).unwrap();

        assert!(!shards.is_empty());
        assert_eq!(shards.len(), TOTAL_SHARDS);
        assert_ne!(request_id, [0u8; 32]);

        // Direct mode: all shards have empty headers and 0 hops
        for shard in &shards {
            assert!(shard.header.is_empty());
            assert_eq!(shard.total_hops, 0);
            assert_eq!(shard.hops_remaining, 0);
        }
    }
}

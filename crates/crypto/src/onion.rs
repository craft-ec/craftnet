//! Onion routing cryptography
//!
//! Builds and peels multi-layer onion headers using X25519 ECDH + ChaCha20-Poly1305.
//! Each relay peels one layer to learn the next hop and settlement data.

use tunnelcraft_core::{ExitPayload, OnionLayer, OnionSettlement, PublicKey, RoutingTag, Id};

use crate::encrypt::{encrypt_for_recipient, decrypt_from_sender, EncryptError};
use crate::keys::EncryptionKeypair;

/// Build a multi-layer onion header for a path of relay hops ending at a destination.
///
/// # Arguments
/// * `hops` - Relay hops (first to last), each with (peer_id_bytes, encryption_pubkey)
/// * `destination` - Final destination (peer_id_bytes, encryption_pubkey)
/// * `settlement_per_hop` - Settlement data for each hop (len must equal hops.len())
/// * `tunnel_id` - If present, included in the innermost (destination/gateway) layer
///
/// # Returns
/// * `(header_bytes, outermost_ephemeral_pubkey)` — the header to put on the shard
///   and the ephemeral pubkey for the first relay's ECDH.
pub fn build_onion_header(
    hops: &[(&[u8], &[u8; 32])],
    destination: (&[u8], &[u8; 32]),
    settlement_per_hop: &[OnionSettlement],
    tunnel_id: Option<&Id>,
) -> Result<(Vec<u8>, [u8; 32]), EncryptError> {
    assert_eq!(hops.len(), settlement_per_hop.len());

    if hops.is_empty() {
        // Direct mode: no relay hops. Return empty header.
        // The shard goes directly to exit, no onion layers to peel.
        return Ok((vec![], [0u8; 32]));
    }

    // Build from innermost to outermost.
    // The innermost layer is for the last relay, pointing to the destination.

    // Generate ephemeral key for exit/destination (the last relay's layer says
    // "forward to destination using this ephemeral key")
    let dest_ephemeral = EncryptionKeypair::generate();

    // Start with the innermost layer (last relay → destination)
    let last_idx = hops.len() - 1;
    let innermost_layer = OnionLayer {
        next_peer_id: destination.0.to_vec(),
        next_ephemeral_pubkey: dest_ephemeral.public_key_bytes(),
        settlement: settlement_per_hop[last_idx].clone(),
        remaining_header: vec![], // No more layers
        is_terminal: true,
        tunnel_id: tunnel_id.copied(),
    };

    let innermost_bytes = innermost_layer.to_bytes()
        .map_err(|_| EncryptError::EncryptionFailed)?;

    // Encrypt innermost for the last relay
    let last_relay_ephemeral = EncryptionKeypair::generate();
    let mut current_encrypted = encrypt_for_recipient(
        hops[last_idx].1,
        &last_relay_ephemeral.secret_key_bytes(),
        &innermost_bytes,
    )?;
    let mut current_ephemeral_pubkey = last_relay_ephemeral.public_key_bytes();

    // Wrap outward: for each hop from second-to-last to first
    for i in (0..last_idx).rev() {
        let next_hop_idx = i + 1;
        let layer = OnionLayer {
            next_peer_id: hops[next_hop_idx].0.to_vec(),
            next_ephemeral_pubkey: current_ephemeral_pubkey,
            settlement: settlement_per_hop[i].clone(),
            remaining_header: current_encrypted,
            is_terminal: false,
            tunnel_id: None,
        };

        let layer_bytes = layer.to_bytes()
            .map_err(|_| EncryptError::EncryptionFailed)?;

        let hop_ephemeral = EncryptionKeypair::generate();
        current_encrypted = encrypt_for_recipient(
            hops[i].1,
            &hop_ephemeral.secret_key_bytes(),
            &layer_bytes,
        )?;
        current_ephemeral_pubkey = hop_ephemeral.public_key_bytes();
    }

    Ok((current_encrypted, current_ephemeral_pubkey))
}

/// Peel one onion layer from a shard header.
///
/// The relay uses its encryption secret key and the shard's ephemeral pubkey
/// to derive the shared secret and decrypt its layer.
///
/// # Returns
/// The decrypted OnionLayer containing next_peer_id, settlement, and remaining_header.
pub fn peel_onion_layer(
    our_encryption_secret: &[u8; 32],
    ephemeral_pubkey: &[u8; 32],
    header: &[u8],
) -> Result<OnionLayer, EncryptError> {
    let decrypted = decrypt_from_sender(
        ephemeral_pubkey,
        our_encryption_secret,
        header,
    )?;

    OnionLayer::from_bytes(&decrypted)
        .map_err(|_| EncryptError::DecryptionFailed)
}

/// Encrypt an ExitPayload for the exit node.
///
/// Uses a fresh ephemeral key. Returns `[ephemeral_pubkey: 32][nonce: 12][ciphertext]`.
pub fn encrypt_exit_payload(
    exit_encryption_pubkey: &[u8; 32],
    payload: &ExitPayload,
) -> Result<Vec<u8>, EncryptError> {
    let payload_bytes = payload.to_bytes()
        .map_err(|_| EncryptError::EncryptionFailed)?;

    let ephemeral = EncryptionKeypair::generate();
    let ciphertext = encrypt_for_recipient(
        exit_encryption_pubkey,
        &ephemeral.secret_key_bytes(),
        &payload_bytes,
    )?;

    // Prepend ephemeral pubkey so exit can ECDH
    let mut result = Vec::with_capacity(32 + ciphertext.len());
    result.extend_from_slice(&ephemeral.public_key_bytes());
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypt an ExitPayload.
///
/// Input format: `[ephemeral_pubkey: 32][nonce: 12][ciphertext]`
pub fn decrypt_exit_payload(
    our_encryption_secret: &[u8; 32],
    data: &[u8],
) -> Result<ExitPayload, EncryptError> {
    if data.len() < 32 {
        return Err(EncryptError::CiphertextTooShort);
    }

    let ephemeral_pubkey: [u8; 32] = data[..32].try_into()
        .map_err(|_| EncryptError::InvalidKey)?;
    let ciphertext = &data[32..];

    let decrypted = decrypt_from_sender(
        &ephemeral_pubkey,
        our_encryption_secret,
        ciphertext,
    )?;

    ExitPayload::from_bytes(&decrypted)
        .map_err(|_| EncryptError::DecryptionFailed)
}

/// Encrypt a routing tag (assembly_id + shard/chunk metadata) for the exit.
///
/// Each call uses a fresh ephemeral key to prevent cross-shard correlation by relays.
/// Returns: `[ephemeral_pubkey: 32][nonce: 12][encrypted(RoutingTag)]`
pub fn encrypt_routing_tag(
    exit_encryption_pubkey: &[u8; 32],
    assembly_id: &Id,
    shard_index: u8,
    total_shards: u8,
    chunk_index: u16,
    total_chunks: u16,
    pool_pubkey: &PublicKey,
) -> Result<Vec<u8>, EncryptError> {
    let tag = RoutingTag {
        assembly_id: *assembly_id,
        shard_index,
        total_shards,
        chunk_index,
        total_chunks,
        pool_pubkey: *pool_pubkey,
    };
    let tag_bytes = tag.to_bytes()
        .map_err(|_| EncryptError::EncryptionFailed)?;

    let ephemeral = EncryptionKeypair::generate();
    let ciphertext = encrypt_for_recipient(
        exit_encryption_pubkey,
        &ephemeral.secret_key_bytes(),
        &tag_bytes,
    )?;

    let mut result = Vec::with_capacity(32 + ciphertext.len());
    result.extend_from_slice(&ephemeral.public_key_bytes());
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypt a routing tag to get the full RoutingTag (assembly_id + metadata).
///
/// Input format: `[ephemeral_pubkey: 32][nonce: 12][encrypted(RoutingTag)]`
pub fn decrypt_routing_tag(
    our_encryption_secret: &[u8; 32],
    tag: &[u8],
) -> Result<RoutingTag, EncryptError> {
    if tag.len() < 32 {
        return Err(EncryptError::CiphertextTooShort);
    }

    let ephemeral_pubkey: [u8; 32] = tag[..32].try_into()
        .map_err(|_| EncryptError::InvalidKey)?;
    let ciphertext = &tag[32..];

    let decrypted = decrypt_from_sender(
        &ephemeral_pubkey,
        our_encryption_secret,
        ciphertext,
    )?;

    RoutingTag::from_bytes(&decrypted)
        .map_err(|_| EncryptError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunnelcraft_core::{LeaseSet, OnionSettlement, ShardType};

    fn make_settlement(idx: u8) -> OnionSettlement {
        OnionSettlement {
            shard_id: [idx + 100; 32],
            payload_size: 1024,
            pool_pubkey: [0u8; 32],
        }
    }

    #[test]
    fn test_routing_tag_roundtrip() {
        let exit_keys = EncryptionKeypair::generate();
        let assembly_id = [77u8; 32];

        let pool_pubkey = [55u8; 32];
        let encrypted = encrypt_routing_tag(
            &exit_keys.public_key_bytes(),
            &assembly_id,
            2, 5, 1, 3,
            &pool_pubkey,
        ).unwrap();

        let tag = decrypt_routing_tag(
            &exit_keys.secret_key_bytes(),
            &encrypted,
        ).unwrap();

        assert_eq!(tag.assembly_id, assembly_id);
        assert_eq!(tag.shard_index, 2);
        assert_eq!(tag.total_shards, 5);
        assert_eq!(tag.chunk_index, 1);
        assert_eq!(tag.total_chunks, 3);
        assert_eq!(tag.pool_pubkey, pool_pubkey);
    }

    #[test]
    fn test_routing_tag_different_ephemeral_keys() {
        let exit_keys = EncryptionKeypair::generate();
        let assembly_id = [77u8; 32];

        let pool_pubkey = [0u8; 32];
        let tag1 = encrypt_routing_tag(&exit_keys.public_key_bytes(), &assembly_id, 0, 5, 0, 1, &pool_pubkey).unwrap();
        let tag2 = encrypt_routing_tag(&exit_keys.public_key_bytes(), &assembly_id, 0, 5, 0, 1, &pool_pubkey).unwrap();

        // Different ephemeral keys → different ciphertext (no correlation)
        assert_ne!(tag1, tag2);

        // Both decrypt to same data
        let rt1 = decrypt_routing_tag(&exit_keys.secret_key_bytes(), &tag1).unwrap();
        let rt2 = decrypt_routing_tag(&exit_keys.secret_key_bytes(), &tag2).unwrap();
        assert_eq!(rt1.assembly_id, assembly_id);
        assert_eq!(rt2.assembly_id, assembly_id);
    }

    #[test]
    fn test_exit_payload_roundtrip() {
        let exit_keys = EncryptionKeypair::generate();

        let payload = ExitPayload {
            request_id: [1u8; 32],
            user_pubkey: [2u8; 32],
            lease_set: LeaseSet::new([4u8; 32]),
            total_hops: 2,
            shard_type: ShardType::Request,
            mode: 0x01,
            data: vec![5, 6, 7, 8, 9],
            response_enc_pubkey: [0u8; 32],
        };

        let encrypted = encrypt_exit_payload(
            &exit_keys.public_key_bytes(),
            &payload,
        ).unwrap();

        let decrypted = decrypt_exit_payload(
            &exit_keys.secret_key_bytes(),
            &encrypted,
        ).unwrap();

        assert_eq!(decrypted.request_id, [1u8; 32]);
        assert_eq!(decrypted.user_pubkey, [2u8; 32]);
        assert_eq!(decrypted.total_hops, 2);
        assert_eq!(decrypted.mode, 0x01);
        assert_eq!(decrypted.data, vec![5, 6, 7, 8, 9]);
    }

    #[test]
    fn test_onion_header_1_hop() {
        let relay1 = EncryptionKeypair::generate();
        let exit = EncryptionKeypair::generate();
        let relay1_peer_id = b"relay1_peer_id";

        let settlement = vec![make_settlement(1)];

        let (header, ephemeral) = build_onion_header(
            &[(relay1_peer_id.as_slice(), &relay1.public_key_bytes())],
            (b"exit_peer_id".as_slice(), &exit.public_key_bytes()),
            &settlement,
            None,
        ).unwrap();

        // Peel the single layer
        let layer = peel_onion_layer(
            &relay1.secret_key_bytes(),
            &ephemeral,
            &header,
        ).unwrap();

        assert_eq!(layer.next_peer_id, b"exit_peer_id");
        assert!(layer.is_terminal);
        assert!(layer.tunnel_id.is_none());
        assert!(layer.remaining_header.is_empty());
        assert_eq!(layer.settlement.payload_size, 1024);
    }

    #[test]
    fn test_onion_header_2_hops() {
        let relay1 = EncryptionKeypair::generate();
        let relay2 = EncryptionKeypair::generate();
        let exit = EncryptionKeypair::generate();

        let settlement = vec![make_settlement(1), make_settlement(2)];

        let (header, ephemeral) = build_onion_header(
            &[
                (b"relay1".as_slice(), &relay1.public_key_bytes()),
                (b"relay2".as_slice(), &relay2.public_key_bytes()),
            ],
            (b"exit".as_slice(), &exit.public_key_bytes()),
            &settlement,
            None,
        ).unwrap();

        // Relay 1 peels
        let layer1 = peel_onion_layer(
            &relay1.secret_key_bytes(),
            &ephemeral,
            &header,
        ).unwrap();

        assert_eq!(layer1.next_peer_id, b"relay2");
        assert!(!layer1.is_terminal);
        assert!(!layer1.remaining_header.is_empty());

        // Relay 2 peels
        let layer2 = peel_onion_layer(
            &relay2.secret_key_bytes(),
            &layer1.next_ephemeral_pubkey,
            &layer1.remaining_header,
        ).unwrap();

        assert_eq!(layer2.next_peer_id, b"exit");
        assert!(layer2.is_terminal);
        assert!(layer2.remaining_header.is_empty());
    }

    #[test]
    fn test_onion_header_3_hops() {
        let relay1 = EncryptionKeypair::generate();
        let relay2 = EncryptionKeypair::generate();
        let relay3 = EncryptionKeypair::generate();
        let exit = EncryptionKeypair::generate();

        let settlement = vec![make_settlement(1), make_settlement(2), make_settlement(3)];

        let (header, ephemeral) = build_onion_header(
            &[
                (b"r1".as_slice(), &relay1.public_key_bytes()),
                (b"r2".as_slice(), &relay2.public_key_bytes()),
                (b"r3".as_slice(), &relay3.public_key_bytes()),
            ],
            (b"exit".as_slice(), &exit.public_key_bytes()),
            &settlement,
            None,
        ).unwrap();

        let l1 = peel_onion_layer(&relay1.secret_key_bytes(), &ephemeral, &header).unwrap();
        assert_eq!(l1.next_peer_id, b"r2");
        assert!(!l1.is_terminal);

        let l2 = peel_onion_layer(&relay2.secret_key_bytes(), &l1.next_ephemeral_pubkey, &l1.remaining_header).unwrap();
        assert_eq!(l2.next_peer_id, b"r3");
        assert!(!l2.is_terminal);

        let l3 = peel_onion_layer(&relay3.secret_key_bytes(), &l2.next_ephemeral_pubkey, &l2.remaining_header).unwrap();
        assert_eq!(l3.next_peer_id, b"exit");
        assert!(l3.is_terminal);
        assert!(l3.remaining_header.is_empty());
    }

    #[test]
    fn test_onion_header_with_tunnel_id() {
        let relay1 = EncryptionKeypair::generate();
        let gateway = EncryptionKeypair::generate();
        let tunnel_id = [42u8; 32];

        let settlement = vec![make_settlement(1)];

        let (header, ephemeral) = build_onion_header(
            &[(b"relay1".as_slice(), &relay1.public_key_bytes())],
            (b"gateway".as_slice(), &gateway.public_key_bytes()),
            &settlement,
            Some(&tunnel_id),
        ).unwrap();

        let layer = peel_onion_layer(
            &relay1.secret_key_bytes(),
            &ephemeral,
            &header,
        ).unwrap();

        assert!(layer.is_terminal);
        assert_eq!(layer.tunnel_id, Some(tunnel_id));
    }

    #[test]
    fn test_direct_mode_empty_header() {
        let exit = EncryptionKeypair::generate();

        let (header, ephemeral) = build_onion_header(
            &[],
            (b"exit".as_slice(), &exit.public_key_bytes()),
            &[],
            None,
        ).unwrap();

        assert!(header.is_empty());
        assert_eq!(ephemeral, [0u8; 32]);
    }

    #[test]
    fn test_wrong_key_cannot_peel() {
        let relay1 = EncryptionKeypair::generate();
        let wrong_key = EncryptionKeypair::generate();
        let exit = EncryptionKeypair::generate();

        let settlement = vec![make_settlement(1)];

        let (header, ephemeral) = build_onion_header(
            &[(b"relay1".as_slice(), &relay1.public_key_bytes())],
            (b"exit".as_slice(), &exit.public_key_bytes()),
            &settlement,
            None,
        ).unwrap();

        // Wrong key cannot peel
        let result = peel_onion_layer(
            &wrong_key.secret_key_bytes(),
            &ephemeral,
            &header,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_exit_payload_wrong_key() {
        let exit_keys = EncryptionKeypair::generate();
        let wrong_keys = EncryptionKeypair::generate();

        let payload = ExitPayload {
            request_id: [1u8; 32],
            user_pubkey: [2u8; 32],
            lease_set: LeaseSet::new([4u8; 32]),
            total_hops: 2,
            shard_type: ShardType::Request,
            mode: 0x00,
            data: vec![],
            response_enc_pubkey: [0u8; 32],
        };

        let encrypted = encrypt_exit_payload(
            &exit_keys.public_key_bytes(),
            &payload,
        ).unwrap();

        let result = decrypt_exit_payload(
            &wrong_keys.secret_key_bytes(),
            &encrypted,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_routing_tag_wrong_key() {
        let exit_keys = EncryptionKeypair::generate();
        let wrong_keys = EncryptionKeypair::generate();

        let tag = encrypt_routing_tag(
            &exit_keys.public_key_bytes(),
            &[1u8; 32],
            0, 5, 0, 1,
            &[0u8; 32],
        ).unwrap();

        let result = decrypt_routing_tag(
            &wrong_keys.secret_key_bytes(),
            &tag,
        );
        assert!(result.is_err());
    }
}

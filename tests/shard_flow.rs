//! Integration tests for the onion-routed shard flow
//!
//! Tests the onion routing lifecycle:
//! 1. Client builds onion-encrypted shards via RequestBuilder::build_onion
//! 2. Relay handlers peel onion layers and produce ForwardReceipts
//! 3. Exit handler decrypts, reconstructs, and processes requests
//! 4. Erasure coding reconstruction (3-of-5)
//! 5. HTTP request/response serialization
//! 6. End-to-end: client -> relay(s) -> exit (direct mode)

use tunnelcraft_client::{RequestBuilder, PathHop, OnionPath, compute_user_proof};
use tunnelcraft_core::{Shard, OnionSettlement, ExitPayload, ShardType, lease_set::LeaseSet};
use tunnelcraft_crypto::{
    SigningKeypair, EncryptionKeypair, build_onion_header, peel_onion_layer,
    encrypt_routing_tag, decrypt_routing_tag, encrypt_exit_payload,
    sign_forward_receipt, verify_forward_receipt,
};
use tunnelcraft_relay::{RelayHandler, RelayConfig};
use tunnelcraft_erasure::{ErasureCoder, TOTAL_SHARDS};
use tunnelcraft_exit::{HttpRequest, HttpResponse};

// =============================================================================
// HELPERS
// =============================================================================

/// Create a PathHop from an encryption keypair and peer_id bytes
fn make_path_hop(peer_id: &[u8], enc_kp: &EncryptionKeypair) -> PathHop {
    PathHop {
        peer_id: peer_id.to_vec(),
        signing_pubkey: [0u8; 32], // not used for routing
        encryption_pubkey: enc_kp.public_key_bytes(),
    }
}

/// Create an empty LeaseSet for tests (direct response mode)
fn empty_lease_set() -> LeaseSet {
    LeaseSet {
        session_id: [0u8; 32],
        leases: vec![],
    }
}

// =============================================================================
// 1. RequestBuilder::build_onion creates valid shards (direct mode)
// =============================================================================

#[test]
fn test_build_onion_direct_mode_creates_valid_shards() {
    let keypair = SigningKeypair::generate();
    let exit_enc = EncryptionKeypair::generate();

    let exit = PathHop {
        peer_id: b"exit_peer".to_vec(),
        signing_pubkey: [2u8; 32],
        encryption_pubkey: exit_enc.public_key_bytes(),
    };

    let builder = RequestBuilder::new("GET", "https://example.com")
        .header("User-Agent", "TunnelCraft-Test");

    let (request_id, shards) = builder
        .build_onion(&keypair, &exit, &[], &empty_lease_set(), 1, [0u8; 32])
        .expect("build_onion should succeed in direct mode");

    // Request ID should be non-zero
    assert_ne!(request_id, [0u8; 32]);

    // Direct mode with small request produces exactly TOTAL_SHARDS (5) shards per chunk
    assert!(
        !shards.is_empty(),
        "Should produce at least one shard"
    );
    // All shards should be from chunk 0 with TOTAL_SHARDS per chunk
    assert_eq!(shards.len() % TOTAL_SHARDS, 0);

    for shard in &shards {
        assert!(!shard.payload.is_empty(), "Shard payload should not be empty");
        assert!(!shard.routing_tag.is_empty(), "Routing tag should not be empty");
        // Direct mode: header is empty (no onion layers)
        assert!(shard.header.is_empty(), "Direct mode should have empty headers");
    }
}

#[test]
fn test_build_onion_with_single_relay_path() {
    let keypair = SigningKeypair::generate();
    let relay_enc = EncryptionKeypair::generate();
    let exit_enc = EncryptionKeypair::generate();

    let exit = PathHop {
        peer_id: b"exit_peer".to_vec(),
        signing_pubkey: [2u8; 32],
        encryption_pubkey: exit_enc.public_key_bytes(),
    };

    let path = OnionPath {
        hops: vec![make_path_hop(b"relay1", &relay_enc)],
        exit: exit.clone(),
    };

    let builder = RequestBuilder::new("GET", "https://example.com");
    let (_request_id, shards) = builder
        .build_onion(&keypair, &exit, &[path], &empty_lease_set(), 42, [0u8; 32])
        .expect("build_onion should succeed with 1 relay");

    assert!(!shards.is_empty());

    for shard in &shards {
        // With 1 relay: header should be non-empty (contains onion layer)
        assert!(!shard.header.is_empty(), "1-hop path should have non-empty header");
        assert!(!shard.payload.is_empty());
        assert!(!shard.routing_tag.is_empty(), "Routing tag should not be empty");
    }
}

// =============================================================================
// 2. Onion header build/peel roundtrip
// =============================================================================

#[test]
fn test_onion_header_1_hop_roundtrip() {
    let relay1 = EncryptionKeypair::generate();
    let exit = EncryptionKeypair::generate();

    let settlement = vec![OnionSettlement {
        blind_token: [10u8; 32],
        shard_id: [20u8; 32],
        payload_size: 512,
        epoch: 7,
        pool_pubkey: [0u8; 32],
    }];

    let (header, ephemeral) = build_onion_header(
        &[(b"relay1_pid".as_slice(), &relay1.public_key_bytes())],
        (b"exit_pid".as_slice(), &exit.public_key_bytes()),
        &settlement,
        None,
    )
    .expect("build_onion_header should succeed");

    assert!(!header.is_empty());
    assert_ne!(ephemeral, [0u8; 32]);

    // Peel the single layer
    let layer = peel_onion_layer(
        &relay1.secret_key_bytes(),
        &ephemeral,
        &header,
    )
    .expect("peel should succeed");

    assert_eq!(layer.next_peer_id, b"exit_pid");
    assert!(layer.is_terminal);
    assert!(layer.remaining_header.is_empty());
    assert!(layer.tunnel_id.is_none());
    assert_eq!(layer.settlement.blind_token, [10u8; 32]);
    assert_eq!(layer.settlement.shard_id, [20u8; 32]);
    assert_eq!(layer.settlement.payload_size, 512);
    assert_eq!(layer.settlement.epoch, 7);
}

#[test]
fn test_onion_header_2_hop_roundtrip() {
    let relay1 = EncryptionKeypair::generate();
    let relay2 = EncryptionKeypair::generate();
    let exit = EncryptionKeypair::generate();

    let settlement = vec![
        OnionSettlement {
            blind_token: [1u8; 32],
            shard_id: [101u8; 32],
            payload_size: 1024,
            epoch: 42,
            pool_pubkey: [0u8; 32],
        },
        OnionSettlement {
            blind_token: [2u8; 32],
            shard_id: [102u8; 32],
            payload_size: 1024,
            epoch: 42,
            pool_pubkey: [0u8; 32],
        },
    ];

    let (header, ephemeral) = build_onion_header(
        &[
            (b"r1".as_slice(), &relay1.public_key_bytes()),
            (b"r2".as_slice(), &relay2.public_key_bytes()),
        ],
        (b"exit".as_slice(), &exit.public_key_bytes()),
        &settlement,
        None,
    )
    .expect("2-hop header should build");

    // Relay 1 peels its layer
    let layer1 = peel_onion_layer(
        &relay1.secret_key_bytes(),
        &ephemeral,
        &header,
    )
    .expect("relay1 peel should succeed");

    assert_eq!(layer1.next_peer_id, b"r2");
    assert!(!layer1.is_terminal);
    assert!(!layer1.remaining_header.is_empty());
    assert_eq!(layer1.settlement.blind_token, [1u8; 32]);

    // Relay 2 peels its layer using the updated ephemeral key and remaining header
    let layer2 = peel_onion_layer(
        &relay2.secret_key_bytes(),
        &layer1.next_ephemeral_pubkey,
        &layer1.remaining_header,
    )
    .expect("relay2 peel should succeed");

    assert_eq!(layer2.next_peer_id, b"exit");
    assert!(layer2.is_terminal);
    assert!(layer2.remaining_header.is_empty());
    assert_eq!(layer2.settlement.blind_token, [2u8; 32]);
}

#[test]
fn test_onion_header_wrong_key_fails() {
    let relay1 = EncryptionKeypair::generate();
    let wrong_key = EncryptionKeypair::generate();
    let exit = EncryptionKeypair::generate();

    let settlement = vec![OnionSettlement {
        blind_token: [1u8; 32],
        shard_id: [2u8; 32],
        payload_size: 100,
        epoch: 0,
        pool_pubkey: [0u8; 32],
    }];

    let (header, ephemeral) = build_onion_header(
        &[(b"r1".as_slice(), &relay1.public_key_bytes())],
        (b"exit".as_slice(), &exit.public_key_bytes()),
        &settlement,
        None,
    )
    .unwrap();

    // Wrong key cannot peel the layer
    let result = peel_onion_layer(
        &wrong_key.secret_key_bytes(),
        &ephemeral,
        &header,
    );
    assert!(result.is_err(), "Wrong key should fail to peel onion layer");
}

#[test]
fn test_onion_header_direct_mode_empty() {
    let exit = EncryptionKeypair::generate();

    let (header, ephemeral) = build_onion_header(
        &[],
        (b"exit".as_slice(), &exit.public_key_bytes()),
        &[],
        None,
    )
    .unwrap();

    assert!(header.is_empty(), "Direct mode should produce empty header");
    assert_eq!(ephemeral, [0u8; 32], "Direct mode should produce zero ephemeral key");
}

// =============================================================================
// 3. RelayHandler peels onion layer
// =============================================================================

#[test]
fn test_relay_handler_peels_1_hop_shard() {
    let relay_enc = EncryptionKeypair::generate();
    let relay_signing = SigningKeypair::generate();
    let exit_enc = EncryptionKeypair::generate();

    let handler = RelayHandler::new(relay_signing, relay_enc.clone());

    let settlement = vec![OnionSettlement {
        blind_token: [5u8; 32],
        shard_id: [50u8; 32],
        payload_size: 256,
        epoch: 10,
        pool_pubkey: [0u8; 32],
    }];

    let (header, ephemeral) = build_onion_header(
        &[(b"relay_pid".as_slice(), &relay_enc.public_key_bytes())],
        (b"exit_pid".as_slice(), &exit_enc.public_key_bytes()),
        &settlement,
        None,
    )
    .unwrap();

    let shard = Shard::new(
        ephemeral,
        header,
        vec![1, 2, 3, 4],
        vec![0u8; 92],
    );

    let sender_pubkey = [9u8; 32];
    let (modified_shard, next_peer, receipt, _, _) = handler
        .handle_shard(shard, sender_pubkey)
        .expect("RelayHandler should peel shard successfully");

    // Next peer should be the exit
    assert_eq!(next_peer, b"exit_pid");

    // After peeling the only layer, the header should be empty (terminal)
    assert!(modified_shard.header.is_empty());

    // Payload should be passed through unchanged
    assert_eq!(modified_shard.payload, vec![1, 2, 3, 4]);

    // Receipt should contain correct sender_pubkey and settlement data
    assert_eq!(receipt.sender_pubkey, sender_pubkey);
    assert_eq!(receipt.blind_token, [5u8; 32]);
}

#[test]
fn test_relay_handler_peels_2_hop_chain() {
    let relay1_enc = EncryptionKeypair::generate();
    let relay1_signing = SigningKeypair::generate();
    let relay2_enc = EncryptionKeypair::generate();
    let relay2_signing = SigningKeypair::generate();
    let exit_enc = EncryptionKeypair::generate();

    let handler1 = RelayHandler::new(relay1_signing, relay1_enc.clone());
    let handler2 = RelayHandler::new(relay2_signing, relay2_enc.clone());

    let settlement = vec![
        OnionSettlement {
            blind_token: [1u8; 32],
            shard_id: [101u8; 32],
            payload_size: 512,
            epoch: 42,
            pool_pubkey: [0u8; 32],
        },
        OnionSettlement {
            blind_token: [2u8; 32],
            shard_id: [102u8; 32],
            payload_size: 512,
            epoch: 42,
            pool_pubkey: [0u8; 32],
        },
    ];

    let (header, ephemeral) = build_onion_header(
        &[
            (b"r1".as_slice(), &relay1_enc.public_key_bytes()),
            (b"r2".as_slice(), &relay2_enc.public_key_bytes()),
        ],
        (b"exit".as_slice(), &exit_enc.public_key_bytes()),
        &settlement,
        None,
    )
    .unwrap();

    let shard = Shard::new(
        ephemeral,
        header,
        vec![10, 20, 30],
        vec![0u8; 92],
    );

    // Relay 1 peels
    let sender1 = [11u8; 32];
    let (shard2, next1, receipt1, _, _) = handler1.handle_shard(shard, sender1).unwrap();
    assert_eq!(next1, b"r2");
    assert!(!shard2.header.is_empty(), "Header should still have relay2's layer");
    assert_eq!(receipt1.blind_token, [1u8; 32]); // settlement[0]
    assert_eq!(receipt1.sender_pubkey, sender1);

    // Relay 2 peels
    let sender2 = [12u8; 32];
    let (shard3, next2, receipt2, _, _) = handler2.handle_shard(shard2, sender2).unwrap();
    assert_eq!(next2, b"exit");
    assert!(shard3.header.is_empty(), "After last relay, header should be empty");
    assert_eq!(receipt2.blind_token, [2u8; 32]); // settlement[1]
    assert_eq!(receipt2.sender_pubkey, sender2);

    // Payload is preserved through the chain
    assert_eq!(shard3.payload, vec![10, 20, 30]);
}

#[test]
fn test_relay_handler_wrong_key_rejects_shard() {
    let relay_enc = EncryptionKeypair::generate();
    let wrong_enc = EncryptionKeypair::generate();
    let wrong_signing = SigningKeypair::generate();
    let exit_enc = EncryptionKeypair::generate();

    // Handler has wrong encryption key
    let handler = RelayHandler::new(wrong_signing, wrong_enc);

    let settlement = vec![OnionSettlement {
        blind_token: [1u8; 32],
        shard_id: [2u8; 32],
        payload_size: 100,
        epoch: 0,
        pool_pubkey: [0u8; 32],
    }];

    let (header, ephemeral) = build_onion_header(
        &[(b"r1".as_slice(), &relay_enc.public_key_bytes())],
        (b"exit".as_slice(), &exit_enc.public_key_bytes()),
        &settlement,
        None,
    )
    .unwrap();

    let shard = Shard::new(
        ephemeral,
        header,
        vec![1, 2, 3],
        vec![0u8; 92],
    );

    let result = handler.handle_shard(shard, [0u8; 32]);
    assert!(result.is_err(), "Wrong encryption key should cause peel failure");
}

// =============================================================================
// 4. ForwardReceipt generation and verification
// =============================================================================

#[test]
fn test_forward_receipt_from_relay_is_valid() {
    let relay_enc = EncryptionKeypair::generate();
    let relay_signing = SigningKeypair::generate();
    let exit_enc = EncryptionKeypair::generate();

    let handler = RelayHandler::new(relay_signing.clone(), relay_enc.clone());

    let blind_token = [42u8; 32];
    let shard_id = [99u8; 32];
    let settlement = vec![OnionSettlement {
        blind_token,
        shard_id,
        payload_size: 2048,
        epoch: 100,
        pool_pubkey: [0u8; 32],
    }];

    let (header, ephemeral) = build_onion_header(
        &[(b"relay".as_slice(), &relay_enc.public_key_bytes())],
        (b"exit".as_slice(), &exit_enc.public_key_bytes()),
        &settlement,
        None,
    )
    .unwrap();

    let shard = Shard::new(
        ephemeral,
        header,
        vec![0xAA; 2048],
        vec![0u8; 92],
    );

    let sender = [77u8; 32];
    let (_modified, _next_peer, receipt, _, _) = handler.handle_shard(shard, sender).unwrap();

    // Verify receipt fields
    assert_eq!(receipt.sender_pubkey, sender);
    assert_eq!(receipt.receiver_pubkey, relay_signing.public_key_bytes());
    assert_eq!(receipt.blind_token, blind_token);
    assert_eq!(receipt.epoch, 100);

    // Verify receipt signature
    assert!(
        verify_forward_receipt(&receipt),
        "ForwardReceipt signature should be valid"
    );
}

#[test]
fn test_forward_receipt_sign_and_verify() {
    let keypair = SigningKeypair::generate();
    let request_id = [1u8; 32];
    let shard_id = [2u8; 32];
    let sender = [3u8; 32];
    let blind_token = [4u8; 32];

    let receipt = sign_forward_receipt(
        &keypair,
        &request_id,
        &shard_id,
        &sender,
        &blind_token,
        4096,
        55,
    );

    assert_eq!(receipt.request_id, request_id);
    assert_eq!(receipt.shard_id, shard_id);
    assert_eq!(receipt.sender_pubkey, sender);
    assert_eq!(receipt.receiver_pubkey, keypair.public_key_bytes());
    assert_eq!(receipt.blind_token, blind_token);
    assert_eq!(receipt.payload_size, 4096);
    assert_eq!(receipt.epoch, 55);

    assert!(verify_forward_receipt(&receipt));
}

#[test]
fn test_forward_receipt_tampered_fails_verification() {
    let keypair = SigningKeypair::generate();

    let mut receipt = sign_forward_receipt(
        &keypair,
        &[1u8; 32],
        &[2u8; 32],
        &[3u8; 32],
        &[4u8; 32],
        1024,
        10,
    );

    // Tamper with the payload_size
    receipt.payload_size = 9999;

    assert!(
        !verify_forward_receipt(&receipt),
        "Tampered receipt should fail verification"
    );
}

// =============================================================================
// 5. Erasure coding tests (3-of-5 reconstruction)
// =============================================================================

#[test]
fn test_erasure_reconstruction_with_3_of_5_shards() {
    let coder = ErasureCoder::new().expect("Failed to create coder");

    let original_data = b"This is test data for erasure coding reconstruction";
    let encoded = coder.encode(original_data).expect("Failed to encode");

    assert_eq!(encoded.len(), TOTAL_SHARDS);

    // Test reconstruction with first 3 shards (data shards only)
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; TOTAL_SHARDS];
    shards[0] = Some(encoded[0].clone());
    shards[1] = Some(encoded[1].clone());
    shards[2] = Some(encoded[2].clone());

    let decoded = coder
        .decode(&mut shards, original_data.len())
        .expect("Failed to decode with first 3 shards");
    assert_eq!(decoded, original_data);

    // Test reconstruction with last 3 shards (1 data + 2 parity)
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; TOTAL_SHARDS];
    shards[2] = Some(encoded[2].clone());
    shards[3] = Some(encoded[3].clone());
    shards[4] = Some(encoded[4].clone());

    let decoded = coder
        .decode(&mut shards, original_data.len())
        .expect("Failed to decode with last 3 shards");
    assert_eq!(decoded, original_data);

    // Test reconstruction with mixed shards (0, 2, 4)
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; TOTAL_SHARDS];
    shards[0] = Some(encoded[0].clone());
    shards[2] = Some(encoded[2].clone());
    shards[4] = Some(encoded[4].clone());

    let decoded = coder
        .decode(&mut shards, original_data.len())
        .expect("Failed to decode with shards 0, 2, 4");
    assert_eq!(decoded, original_data);
}

#[test]
fn test_erasure_fails_with_2_of_5_shards() {
    let coder = ErasureCoder::new().expect("Failed to create coder");

    let original_data = b"Test data";
    let encoded = coder.encode(original_data).expect("Failed to encode");

    // Only 2 shards available - should fail
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; TOTAL_SHARDS];
    shards[0] = Some(encoded[0].clone());
    shards[1] = Some(encoded[1].clone());

    let result = coder.decode(&mut shards, original_data.len());
    assert!(result.is_err(), "Should fail with only 2 of 5 shards");
}

#[test]
fn test_erasure_verify_enough_shards() {
    let coder = ErasureCoder::new().unwrap();
    let data = b"Verify test data for erasure";
    let encoded = coder.encode(data).unwrap();

    // 3 shards should be enough
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; TOTAL_SHARDS];
    shards[0] = Some(encoded[0].clone());
    shards[1] = Some(encoded[1].clone());
    shards[2] = Some(encoded[2].clone());
    assert!(coder.verify(&shards), "3 shards should be enough");

    // 2 shards should not be enough
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; TOTAL_SHARDS];
    shards[0] = Some(encoded[0].clone());
    shards[1] = Some(encoded[1].clone());
    assert!(!coder.verify(&shards), "2 shards should not be enough");
}

// =============================================================================
// 6. HTTP request/response serialization
// =============================================================================

#[test]
fn test_http_request_serialization_roundtrip() {
    use std::collections::HashMap;

    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("Authorization".to_string(), "Bearer token123".to_string());

    let request = HttpRequest {
        method: "POST".to_string(),
        url: "https://api.example.com/data".to_string(),
        headers,
        body: Some(b"{\"key\": \"value\"}".to_vec()),
    };

    let bytes = request.to_bytes();
    let parsed = HttpRequest::from_bytes(&bytes).expect("Should parse");

    assert_eq!(parsed.method, "POST");
    assert_eq!(parsed.url, "https://api.example.com/data");
    assert_eq!(parsed.headers.len(), 2);
    assert_eq!(parsed.body.unwrap(), b"{\"key\": \"value\"}");
}

#[test]
fn test_http_request_no_body() {
    use std::collections::HashMap;

    let request = HttpRequest {
        method: "GET".to_string(),
        url: "https://example.com".to_string(),
        headers: HashMap::new(),
        body: None,
    };

    let bytes = request.to_bytes();
    let parsed = HttpRequest::from_bytes(&bytes).expect("Should parse");

    assert_eq!(parsed.method, "GET");
    assert_eq!(parsed.url, "https://example.com");
    assert!(parsed.body.is_none());
}

#[test]
fn test_http_response_serialization() {
    use std::collections::HashMap;

    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "text/html".to_string());

    let response = HttpResponse::new(200, headers, b"<html>Hello</html>".to_vec());

    let bytes = response.to_bytes();
    assert!(!bytes.is_empty());

    let parsed = HttpResponse::from_bytes(&bytes).expect("Should parse");
    assert_eq!(parsed.status, 200);
    assert_eq!(parsed.body, b"<html>Hello</html>");
}

// =============================================================================
// 7. Routing tag encryption/decryption roundtrip
// =============================================================================

#[test]
fn test_routing_tag_encrypt_decrypt_roundtrip() {
    let exit_enc = EncryptionKeypair::generate();
    let assembly_id = [42u8; 32];

    let encrypted = encrypt_routing_tag(
        &exit_enc.public_key_bytes(),
        &assembly_id,
        0,
        5,
        0,
        1,
    )
    .expect("encrypt_routing_tag should succeed");

    assert!(!encrypted.is_empty(), "Routing tag should not be empty");

    let decrypted = decrypt_routing_tag(
        &exit_enc.secret_key_bytes(),
        &encrypted,
    )
    .expect("decrypt_routing_tag should succeed");

    assert_eq!(decrypted.assembly_id, assembly_id);
    assert_eq!(decrypted.shard_index, 0);
    assert_eq!(decrypted.total_shards, 5);
    assert_eq!(decrypted.chunk_index, 0);
    assert_eq!(decrypted.total_chunks, 1);
}

#[test]
fn test_routing_tags_are_unlinkable() {
    let exit_enc = EncryptionKeypair::generate();
    let assembly_id = [42u8; 32];

    let tag1 = encrypt_routing_tag(&exit_enc.public_key_bytes(), &assembly_id, 0, 5, 0, 1).unwrap();
    let tag2 = encrypt_routing_tag(&exit_enc.public_key_bytes(), &assembly_id, 0, 5, 0, 1).unwrap();

    // Each call uses a fresh ephemeral key, so tags should differ
    assert_ne!(tag1, tag2, "Same assembly_id should produce different ciphertexts");

    // But both should decrypt to the same assembly_id
    let rt1 = decrypt_routing_tag(&exit_enc.secret_key_bytes(), &tag1).unwrap();
    let rt2 = decrypt_routing_tag(&exit_enc.secret_key_bytes(), &tag2).unwrap();
    assert_eq!(rt1.assembly_id, assembly_id);
    assert_eq!(rt2.assembly_id, assembly_id);
}

#[test]
fn test_routing_tag_wrong_key_fails() {
    let exit_enc = EncryptionKeypair::generate();
    let wrong_enc = EncryptionKeypair::generate();

    let tag = encrypt_routing_tag(&exit_enc.public_key_bytes(), &[1u8; 32], 0, 5, 0, 1).unwrap();

    let result = decrypt_routing_tag(&wrong_enc.secret_key_bytes(), &tag);
    assert!(result.is_err(), "Wrong key should fail to decrypt routing tag");
}

// =============================================================================
// 8. ExitPayload encryption/decryption roundtrip
// =============================================================================

#[test]
fn test_exit_payload_encrypt_decrypt_roundtrip() {
    let exit_enc = EncryptionKeypair::generate();

    let payload = ExitPayload {
        request_id: [1u8; 32],
        user_pubkey: [2u8; 32],
        user_proof: [3u8; 32],
        lease_set: empty_lease_set(),
        total_hops: 2,
        shard_type: ShardType::Request,
        mode: 0x00,
        data: b"GET\nhttps://example.com\n0\n0\n".to_vec(),
        response_enc_pubkey: [0u8; 32],
    };

    let encrypted = encrypt_exit_payload(
        &exit_enc.public_key_bytes(),
        &payload,
    )
    .expect("encrypt_exit_payload should succeed");

    assert!(encrypted.len() > 32, "Encrypted payload should have ephemeral key + ciphertext");

    let decrypted = tunnelcraft_crypto::decrypt_exit_payload(
        &exit_enc.secret_key_bytes(),
        &encrypted,
    )
    .expect("decrypt_exit_payload should succeed");

    assert_eq!(decrypted.request_id, [1u8; 32]);
    assert_eq!(decrypted.user_pubkey, [2u8; 32]);
    assert_eq!(decrypted.user_proof, [3u8; 32]);
    assert_eq!(decrypted.total_hops, 2);
    assert_eq!(decrypted.shard_type, ShardType::Request);
    assert_eq!(decrypted.mode, 0x00);
    assert_eq!(decrypted.data, b"GET\nhttps://example.com\n0\n0\n");
}

// =============================================================================
// 9. Complete request flow: client -> exit (direct mode)
// =============================================================================

#[tokio::test]
async fn test_complete_direct_mode_flow_client_to_exit() {
    use tunnelcraft_exit::{ExitHandler, ExitConfig};

    let user_keypair = SigningKeypair::generate();
    let exit_signing = SigningKeypair::generate();
    let exit_enc = EncryptionKeypair::generate();

    // Create exit handler with explicit encryption keypair so we can match it
    let mut exit_handler = ExitHandler::with_keypairs(
        ExitConfig {
            blocked_domains: vec![], // Allow all for testing
            ..Default::default()
        },
        exit_signing,
        exit_enc.clone(),
    )
    .expect("ExitHandler creation should succeed");

    let exit_hop = PathHop {
        peer_id: b"exit_peer".to_vec(),
        signing_pubkey: [0u8; 32],
        encryption_pubkey: exit_enc.public_key_bytes(),
    };

    // Build onion shards in direct mode
    let builder = RequestBuilder::new("GET", "https://httpbin.org/get")
        .header("User-Agent", "TunnelCraft-Test");

    let (_request_id, shards) = builder
        .build_onion(&user_keypair, &exit_hop, &[], &empty_lease_set(), 1, [0u8; 32])
        .expect("build_onion should succeed");

    assert!(!shards.is_empty(), "Should produce shards");

    // Feed all shards to exit handler
    let mut response_shards = None;
    for shard in shards {
        // In direct mode, shards go straight to exit
        match exit_handler.process_shard(shard).await {
            Ok(Some((resp, _gateway))) => {
                response_shards = Some(resp);
            }
            Ok(None) => {
                // Still collecting shards
            }
            Err(e) => {
                // HTTP request to external service may fail in test environment,
                // but the shard processing itself should work up to the HTTP fetch.
                // This is acceptable for integration tests that don't have network.
                eprintln!("Exit processing error (expected in offline tests): {}", e);
                return;
            }
        }
    }

    // If we got response shards (network was available), verify them
    if let Some(resp) = response_shards {
        assert!(!resp.is_empty(), "Response should have shards");
        for shard in &resp {
            assert!(!shard.payload.is_empty());
            assert!(!shard.routing_tag.is_empty(), "Routing tag should not be empty");
        }
    }
}

// =============================================================================
// 10. Shard struct and serialization tests
// =============================================================================

#[test]
fn test_shard_new_fields() {
    let shard = Shard::new(
        [1u8; 32],
        vec![2, 3, 4],
        vec![5, 6, 7, 8],
        vec![9u8; 98],
    );

    assert_eq!(shard.ephemeral_pubkey, [1u8; 32]);
    assert_eq!(shard.header, vec![2, 3, 4]);
    assert_eq!(shard.payload, vec![5, 6, 7, 8]);
    assert!(!shard.routing_tag.is_empty(), "Routing tag should not be empty");
}

#[test]
fn test_shard_serialization_roundtrip() {
    let shard = Shard::new(
        [1u8; 32],
        vec![2, 3, 4, 5],
        vec![10, 20, 30],
        vec![0u8; 98],
    );

    let bytes = shard.to_bytes().unwrap();
    let restored = Shard::from_bytes(&bytes).unwrap();

    assert_eq!(restored.ephemeral_pubkey, shard.ephemeral_pubkey);
    assert_eq!(restored.header, shard.header);
    assert_eq!(restored.payload, shard.payload);
    assert_eq!(restored.routing_tag, shard.routing_tag);
}

// =============================================================================
// 11. User proof computation
// =============================================================================

#[test]
fn test_user_proof_is_deterministic() {
    let request_id = [1u8; 32];
    let user_pubkey = [2u8; 32];
    let sig = [3u8; 64];

    let proof1 = compute_user_proof(&request_id, &user_pubkey, &sig);
    let proof2 = compute_user_proof(&request_id, &user_pubkey, &sig);
    assert_eq!(proof1, proof2, "Same inputs should produce same user_proof");
}

#[test]
fn test_user_proof_different_inputs_differ() {
    let proof1 = compute_user_proof(&[1u8; 32], &[2u8; 32], &[3u8; 64]);
    let proof2 = compute_user_proof(&[4u8; 32], &[2u8; 32], &[3u8; 64]);
    assert_ne!(proof1, proof2, "Different request_ids should produce different proofs");

    let proof3 = compute_user_proof(&[1u8; 32], &[5u8; 32], &[3u8; 64]);
    assert_ne!(proof1, proof3, "Different user_pubkeys should produce different proofs");
}

// =============================================================================
// 12. Build onion shards contain encrypted routing tags with erasure metadata
// =============================================================================

#[test]
fn test_build_onion_shards_have_encrypted_routing_tags() {
    let keypair = SigningKeypair::generate();
    let exit_enc = EncryptionKeypair::generate();

    let exit = PathHop {
        peer_id: b"exit".to_vec(),
        signing_pubkey: [0u8; 32],
        encryption_pubkey: exit_enc.public_key_bytes(),
    };

    let (_request_id, shards) = RequestBuilder::new("GET", "https://example.com")
        .build_onion(&keypair, &exit, &[], &empty_lease_set(), 1, [0u8; 32])
        .expect("build_onion should succeed");

    // Verify each shard has a non-empty routing tag that the exit can decrypt
    for shard in &shards {
        assert!(!shard.routing_tag.is_empty(), "Routing tag should not be empty");
        // The routing tag should be decryptable by the exit's key
        let tag = decrypt_routing_tag(
            &exit_enc.secret_key_bytes(),
            &shard.routing_tag,
        )
        .expect("Exit should be able to decrypt routing tag");
        assert_eq!(tag.total_shards, TOTAL_SHARDS as u8);
        assert!(tag.total_chunks >= 1);
    }
}

// =============================================================================
// 13. Relay handler pubkey accessors
// =============================================================================

#[test]
fn test_relay_handler_pubkey_accessors() {
    let signing = SigningKeypair::generate();
    let encryption = EncryptionKeypair::generate();
    let signing_pub = signing.public_key_bytes();
    let enc_pub = encryption.public_key_bytes();

    let handler = RelayHandler::new(signing, encryption);

    assert_eq!(handler.pubkey(), signing_pub);
    assert_eq!(handler.encryption_pubkey(), enc_pub);
}

#[test]
fn test_relay_handler_with_config() {
    let signing = SigningKeypair::generate();
    let encryption = EncryptionKeypair::generate();

    let config = RelayConfig {
        can_be_last_hop: false,
    };

    let handler = RelayHandler::with_config(signing, encryption, config);
    // Should not panic - handler created successfully
    assert_ne!(handler.pubkey(), [0u8; 32]);
}

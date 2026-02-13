//! Request building and onion shard creation
//!
//! Builds ExitPayload, encrypts for exit, erasure-codes, and wraps each
//! piece in an onion-routed Shard with per-hop settlement data.

use sha2::{Sha256, Digest};

use tunnelcraft_core::{
    Shard, Id, PublicKey, ExitPayload, ShardType, OnionSettlement,
    lease_set::LeaseSet,
};
use tunnelcraft_crypto::{
    SigningKeypair, build_onion_header, encrypt_exit_payload, encrypt_routing_tag,
};
use tunnelcraft_erasure::TOTAL_SHARDS;
use tunnelcraft_erasure::chunker::chunk_and_encode;

use crate::path::{OnionPath, PathHop, random_id};
use crate::{ClientError, Result};

/// Builder for creating VPN requests
pub struct RequestBuilder {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

impl RequestBuilder {
    /// Create a new request builder
    pub fn new(method: &str, url: &str) -> Self {
        Self {
            method: method.to_uppercase(),
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    /// Add a header
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    /// Set request body
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    /// Serialize the request to bytes (HTTP format for exit)
    fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(self.method.as_bytes());
        data.push(b'\n');

        data.extend_from_slice(self.url.as_bytes());
        data.push(b'\n');

        data.extend_from_slice(self.headers.len().to_string().as_bytes());
        data.push(b'\n');

        for (key, value) in &self.headers {
            data.extend_from_slice(format!("{}: {}", key, value).as_bytes());
            data.push(b'\n');
        }

        let body_len = self.body.as_ref().map(|b| b.len()).unwrap_or(0);
        data.extend_from_slice(body_len.to_string().as_bytes());
        data.push(b'\n');

        if let Some(body) = &self.body {
            data.extend_from_slice(body);
        }

        data
    }

    /// Build onion-routed request shards.
    ///
    /// # Arguments
    /// * `keypair` - User's signing keypair
    /// * `exit` - Exit node hop info (pubkey + encryption key)
    /// * `paths` - Per-shard onion paths (one per shard, or round-robin)
    /// * `lease_set` - LeaseSet for response routing
    /// * `pool_pubkey` - Ephemeral subscription key or persistent free-tier key
    ///
    /// # Returns
    /// * `(request_id, Vec<Shard>)` — request ID and shards ready to send
    pub fn build_onion(
        self,
        keypair: &SigningKeypair,
        exit: &PathHop,
        paths: &[OnionPath],
        lease_set: &LeaseSet,
        pool_pubkey: PublicKey,
    ) -> Result<(Id, Vec<Shard>)> {
        self.build_onion_with_enc_key(keypair, exit, paths, lease_set, [0u8; 32], pool_pubkey)
    }

    /// Build onion-routed request shards with an explicit X25519 encryption pubkey
    /// for response encryption. The exit uses this key to encrypt responses.
    pub fn build_onion_with_enc_key(
        self,
        keypair: &SigningKeypair,
        exit: &PathHop,
        paths: &[OnionPath],
        lease_set: &LeaseSet,
        response_enc_pubkey: [u8; 32],
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
            mode: 0x00, // HTTP mode
            data: self.serialize(),
            response_enc_pubkey,
        };

        // Encrypt for exit
        let encrypted = encrypt_exit_payload(
            &exit.encryption_pubkey,
            &exit_payload,
        ).map_err(|e| ClientError::CryptoError(e.to_string()))?;

        // Prepend original length (4-byte LE u32) so exit can strip erasure padding
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
                    // Direct mode: no relays
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

                // Encrypt routing tag with shard/chunk metadata (per-shard fresh ephemeral key)
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
                    payload,
                    routing_tag,
                ));
            }
        }

        Ok((request_id, shards))
    }
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
    fn test_request_builder() {
        let builder = RequestBuilder::new("GET", "https://example.com")
            .header("User-Agent", "TunnelCraft");

        assert_eq!(builder.method, "GET");
        assert_eq!(builder.url, "https://example.com");
        assert_eq!(builder.headers.len(), 1);
    }

    #[test]
    fn test_request_serialization() {
        let builder = RequestBuilder::new("POST", "https://api.example.com")
            .header("Content-Type", "application/json")
            .body(b"{\"key\": \"value\"}".to_vec());

        let data = builder.serialize();
        assert!(data.starts_with(b"POST\n"));
        assert!(data.windows(b"application/json".len()).any(|w| w == b"application/json"));
    }

    #[test]
    fn test_build_onion_direct() {
        let keypair = SigningKeypair::generate();
        let enc_keypair = tunnelcraft_crypto::EncryptionKeypair::generate();

        let exit = PathHop {
            peer_id: b"exit_peer".to_vec(),
            signing_pubkey: [2u8; 32],
            encryption_pubkey: enc_keypair.public_key_bytes(),
        };

        let lease_set = LeaseSet {
            session_id: [0u8; 32],
            leases: vec![],
        };

        let builder = RequestBuilder::new("GET", "https://example.com");
        let (request_id, shards) = builder.build_onion(
            &keypair,
            &exit,
            &[], // direct mode
            &lease_set,
            [0u8; 32],
        ).unwrap();

        assert_eq!(shards.len(), TOTAL_SHARDS);
        assert_ne!(request_id, [0u8; 32]);

        // Direct mode: all shards have empty headers
        for shard in &shards {
            assert!(shard.header.is_empty());
            assert!(!shard.routing_tag.is_empty());
        }
    }

    #[test]
    fn test_request_method_normalized_to_uppercase() {
        let builder = RequestBuilder::new("get", "https://example.com");
        assert_eq!(builder.method, "GET");
    }

    #[test]
    fn test_shard_ids_unique() {
        let request_id: Id = [42u8; 32];
        let relay = [1u8; 32];

        let id0 = generate_shard_id(&request_id, 0, 0, &relay);
        let id1 = generate_shard_id(&request_id, 0, 1, &relay);
        let id2 = generate_shard_id(&request_id, 1, 0, &relay);

        assert_ne!(id0, id1);
        assert_ne!(id0, id2);
    }

    #[test]
    fn test_shard_id_deterministic() {
        let request_id: Id = [42u8; 32];
        let relay = [1u8; 32];
        let a = generate_shard_id(&request_id, 0, 0, &relay);
        let b = generate_shard_id(&request_id, 0, 0, &relay);
        assert_eq!(a, b);
    }

    #[test]
    fn test_shard_id_unique_per_relay() {
        let request_id: Id = [42u8; 32];
        let relay_a = [1u8; 32];
        let relay_b = [2u8; 32];
        let id_a = generate_shard_id(&request_id, 0, 0, &relay_a);
        let id_b = generate_shard_id(&request_id, 0, 0, &relay_b);
        assert_ne!(id_a, id_b, "Same shard for different relays should have different shard_ids");
    }

}

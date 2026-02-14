//! Request building and onion shard creation
//!
//! Builds HTTP request data, then delegates to the shared shard builder
//! for encrypt → frame → erasure code → onion wrap.

use tunnelcraft_core::{
    Shard, Id, PublicKey,
    lease_set::LeaseSet,
};
use tunnelcraft_crypto::SigningKeypair;

use crate::path::{OnionPath, PathHop};
use crate::shard_builder::build_onion_shards;
use crate::Result;

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
        build_onion_shards(
            0x00, // HTTP mode
            self.serialize(),
            response_enc_pubkey,
            keypair,
            exit,
            paths,
            lease_set,
            pool_pubkey,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunnelcraft_erasure::TOTAL_SHARDS;

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
            assert_eq!(shard.total_hops, 0);
            assert_eq!(shard.hops_remaining, 0);
        }
    }

    #[test]
    fn test_request_method_normalized_to_uppercase() {
        let builder = RequestBuilder::new("get", "https://example.com");
        assert_eq!(builder.method, "GET");
    }
}

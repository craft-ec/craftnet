use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use tunnelcraft_core::ForwardReceipt;

use crate::keys::SigningKeypair;

/// Sign data with a signing keypair
pub fn sign_data(keypair: &SigningKeypair, data: &[u8]) -> [u8; 64] {
    let signature: Signature = keypair.signing_key.sign(data);
    signature.to_bytes()
}

/// Verify a signature
pub fn verify_signature(pubkey: &[u8; 32], data: &[u8], signature: &[u8; 64]) -> bool {
    let verifying_key = match VerifyingKey::from_bytes(pubkey) {
        Ok(vk) => vk,
        Err(_) => return false,
    };

    let signature = Signature::from_bytes(signature);

    verifying_key.verify(data, &signature).is_ok()
}

/// Sign a forward receipt proving we received a shard.
///
/// The receiving relay calls this to create a cryptographic proof of delivery.
/// The sending relay uses the receipt as on-chain settlement proof.
/// Uses shard_id (unique hash) so request and response shards produce distinct receipts.
///
/// `sender_pubkey` binds this receipt to the forwarding relay (anti-Sybil).
/// `pool_pubkey` is the ephemeral subscription key (or persistent key for free-tier).
/// `payload_size` is the actual payload bytes — settlement weights by bandwidth.
pub fn sign_forward_receipt(
    keypair: &SigningKeypair,
    shard_id: &[u8; 32],
    sender_pubkey: &[u8; 32],
    pool_pubkey: &[u8; 32],
    payload_size: u32,
) -> ForwardReceipt {
    let receiver_pubkey = keypair.public_key_bytes();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let data = ForwardReceipt::signable_data(
        shard_id,
        sender_pubkey,
        &receiver_pubkey,
        pool_pubkey,
        payload_size,
        timestamp,
    );
    let signature = sign_data(keypair, &data);
    ForwardReceipt {
        shard_id: *shard_id,
        sender_pubkey: *sender_pubkey,
        receiver_pubkey,
        pool_pubkey: *pool_pubkey,
        payload_size,
        timestamp,
        signature,
    }
}

/// Verify a forward receipt's signature
pub fn verify_forward_receipt(receipt: &ForwardReceipt) -> bool {
    let data = ForwardReceipt::signable_data(
        &receipt.shard_id,
        &receipt.sender_pubkey,
        &receipt.receiver_pubkey,
        &receipt.pool_pubkey,
        receipt.payload_size,
        receipt.timestamp,
    );
    verify_signature(&receipt.receiver_pubkey, &data, &receipt.signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let keypair = SigningKeypair::generate();
        let data = b"Hello, TunnelCraft!";

        let signature = sign_data(&keypair, data);
        assert!(verify_signature(
            &keypair.public_key_bytes(),
            data,
            &signature
        ));

        // Wrong data should fail
        assert!(!verify_signature(
            &keypair.public_key_bytes(),
            b"Wrong data",
            &signature
        ));
    }

    #[test]
    fn test_wrong_pubkey_fails() {
        let keypair1 = SigningKeypair::generate();
        let keypair2 = SigningKeypair::generate();
        let data = b"Test data";

        let signature = sign_data(&keypair1, data);

        // Verification with wrong pubkey should fail
        assert!(!verify_signature(
            &keypair2.public_key_bytes(),
            data,
            &signature
        ));
    }
}

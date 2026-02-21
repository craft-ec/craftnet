//! Cryptographic helpers for Craftnet structures

use craftec_crypto::{sign_data, verify_signature, SigningKeypair};
use crate::ForwardReceipt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Sign a forward receipt proving we received a shard.
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

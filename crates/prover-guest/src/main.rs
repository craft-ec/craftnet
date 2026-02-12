//! SP1 guest program for TunnelCraft receipt proving.
//!
//! Runs inside the SP1 RISC-V VM. Given a batch of ForwardReceipts:
//! 1. Verifies all receipts have the same sender_pubkey (anti-Sybil)
//! 2. Verifies all receipts have the same epoch (anti-replay)
//! 3. Hashes each receipt into a Merkle leaf (SHA256)
//! 4. Builds a binary Merkle tree from the leaves
//! 5. Commits (root, batch_count, sender_pubkey, epoch) as public values
//!
//! Signature verification is NOT done inside the ZK proof — the pool_id
//! (on-chain subscription) is the trust anchor. Receipt signatures are
//! verified off-chain by the aggregator and are publicly verifiable.

#![no_main]
sp1_zkvm::entrypoint!(main);

use sha2::{Digest, Sha256};

use tunnelcraft_prover_guest_types::{GuestInput, GuestOutput, GuestReceipt};

pub fn main() {
    let input = sp1_zkvm::io::read::<GuestInput>();

    assert!(!input.receipts.is_empty(), "empty batch");

    // 1. All receipts must have the same sender (anti-Sybil) and same epoch (anti-replay)
    let sender = input.receipts[0].sender_pubkey;
    let epoch = input.receipts[0].epoch;
    for r in &input.receipts {
        assert_eq!(
            r.sender_pubkey, sender,
            "all receipts must have the same sender_pubkey"
        );
        assert_eq!(
            r.epoch, epoch,
            "all receipts must have the same epoch"
        );
    }

    // 2. Hash each receipt into a Merkle leaf
    let mut leaves: Vec<[u8; 32]> = Vec::with_capacity(input.receipts.len());
    for receipt in &input.receipts {
        let leaf = receipt_leaf(receipt);
        leaves.push(leaf);
    }

    // 3. Build Merkle root
    let root = merkle_root(&leaves);

    // 4. Commit output as public values
    let output = GuestOutput {
        root,
        batch_count: input.receipts.len() as u64,
        sender_pubkey: sender,
        epoch,
    };
    sp1_zkvm::io::commit(&output);
}

/// Hash a receipt into a Merkle leaf matching StubProver::receipt_leaf().
///
/// SHA256(request_id || shard_id || sender_pubkey || receiver_pubkey || blind_token || payload_size_le || epoch_le || timestamp_le)
fn receipt_leaf(receipt: &GuestReceipt) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&receipt.request_id);
    hasher.update(&receipt.shard_id);
    hasher.update(&receipt.sender_pubkey);
    hasher.update(&receipt.receiver_pubkey);
    hasher.update(&receipt.blind_token);
    hasher.update(&receipt.payload_size.to_le_bytes());
    hasher.update(&receipt.epoch.to_le_bytes());
    hasher.update(&receipt.timestamp.to_le_bytes());
    let result = hasher.finalize();
    let mut leaf = [0u8; 32];
    leaf.copy_from_slice(&result);
    leaf
}

/// Build Merkle root matching MerkleTree::from_leaves().
///
/// Pad to next power of 2 with [0u8; 32], then bottom-up:
///   parent = SHA256(left || right)
fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    if leaves.len() == 1 {
        return leaves[0];
    }

    // Pad to next power of 2
    let n = leaves.len().next_power_of_two();
    let mut nodes: Vec<[u8; 32]> = Vec::with_capacity(n);
    nodes.extend_from_slice(leaves);
    while nodes.len() < n {
        nodes.push([0u8; 32]);
    }

    // Bottom-up merge
    while nodes.len() > 1 {
        let mut next = Vec::with_capacity(nodes.len() / 2);
        for i in (0..nodes.len()).step_by(2) {
            let mut hasher = Sha256::new();
            hasher.update(&nodes[i]);
            hasher.update(&nodes[i + 1]);
            let result = hasher.finalize();
            let mut parent = [0u8; 32];
            parent.copy_from_slice(&result);
            next.push(parent);
        }
        nodes = next;
    }

    nodes[0]
}

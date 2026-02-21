//! SP1 guest program for distribution Merkle tree proving.
//!
//! Runs inside the SP1 RISC-V VM. Given distribution entries:
//! 1. Sorts entries by relay_pubkey (deterministic ordering)
//! 2. Builds Merkle tree: leaf = SHA256(relay_pubkey || cumulative_bytes_le)
//! 3. Pads to next power-of-2 with [0u8; 32], bottom-up SHA256(left || right)
//! 4. Commits output fields individually via commit_slice() for predictable layout
//!
//! The leaf formula matches `merkle_leaf()` in `crates/prover/src/merkle.rs`
//! and `verify_merkle_proof` in the on-chain program.

#![no_main]
sp1_zkvm::entrypoint!(main);

use sha2::{Digest, Sha256};

use craftnet_distribution_guest_types::DistributionInput;

pub fn main() {
    let input = sp1_zkvm::io::read::<DistributionInput>();

    assert!(!input.entries.is_empty(), "empty distribution");

    // 1. Sort entries by relay_pubkey for deterministic ordering
    let mut entries = input.entries.clone();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // 2. Compute total bytes
    let total_bytes: u64 = entries.iter().map(|(_, bytes)| bytes).sum();

    // 3. Build Merkle tree
    let leaves: Vec<[u8; 32]> = entries
        .iter()
        .map(|(pubkey, bytes)| merkle_leaf(pubkey, *bytes))
        .collect();

    let root = merkle_root(&leaves);

    // 4. Commit output fields individually for predictable byte layout
    //    root (32B) + total_bytes (8B LE) + entry_count (4B LE) + pool_pubkey (32B)
    //    = 76 bytes total
    sp1_zkvm::io::commit_slice(&root);
    sp1_zkvm::io::commit_slice(&total_bytes.to_le_bytes());
    sp1_zkvm::io::commit_slice(&(entries.len() as u32).to_le_bytes());
    sp1_zkvm::io::commit_slice(&input.pool_pubkey);
}

/// Compute a leaf hash matching `merkle_leaf()` in `crates/prover/src/merkle.rs`.
///
/// `SHA256(relay_pubkey || cumulative_bytes.to_le_bytes())`
fn merkle_leaf(relay_pubkey: &[u8; 32], relay_bytes: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(relay_pubkey);
    hasher.update(relay_bytes.to_le_bytes());
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Build Merkle root matching `MerkleTree::from_leaves()` in `crates/prover/src/merkle.rs`.
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

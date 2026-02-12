//! Shared types between the distribution guest program and host prover.
//!
//! These types are `no_std`-compatible so they can be used inside the
//! SP1 RISC-V VM guest as well as by the host-side distribution prover.

#![no_std]

extern crate alloc;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// Input to the distribution guest program.
///
/// Contains the relay entries and pool metadata needed to build the
/// distribution Merkle tree inside the zkVM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionInput {
    /// (relay_pubkey, cumulative_bytes) entries
    pub entries: Vec<([u8; 32], u64)>,
    /// Pool pubkey (subscription account)
    pub pool_pubkey: [u8; 32],
    /// Epoch this distribution covers
    pub epoch: u64,
}

/// Output committed by the distribution guest program.
///
/// The on-chain verifier checks these public values against the
/// instruction arguments to ensure the aggregator computed the
/// distribution correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionOutput {
    /// Merkle root of the distribution tree
    pub root: [u8; 32],
    /// Total bytes across all relay entries
    pub total_bytes: u64,
    /// Number of relay entries
    pub entry_count: u32,
    /// Pool pubkey (passed through for binding)
    pub pool_pubkey: [u8; 32],
    /// Epoch (passed through for binding)
    pub epoch: u64,
}

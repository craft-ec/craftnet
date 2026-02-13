//! TunnelCraft Prover
//!
//! Binary Merkle tree, stub receipt prover, and distribution proof generation.
//!
//! The `MerkleTree` is used by both the aggregator (to build distribution
//! roots with proofs for each relay) and by the on-chain program (to
//! verify claims). The `StubProver` hashes receipts into a Merkle tree
//! for ProofMessage chain continuity. The `DistributionProver` generates
//! Groth16 proofs for on-chain distribution verification.

pub mod merkle;
pub mod stub;
pub mod traits;

#[cfg(feature = "sp1")]
pub mod distribution;

pub use merkle::{hash_pair, merkle_leaf, MerkleProof, MerkleTree};
pub use stub::StubProver;
pub use traits::{ProofOutput, Prover, ProverError};

#[cfg(feature = "sp1")]
pub use distribution::{DistributionProver, DistributionGroth16Proof};

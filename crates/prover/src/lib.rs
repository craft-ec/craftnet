//! TunnelCraft Prover
//!
//! Binary Merkle tree and pluggable prover trait for settlement proofs.
//!
//! The `MerkleTree` is used by both the aggregator (to build distribution
//! roots with proofs for each relay) and by the on-chain program (to
//! verify claims). The `Prover` trait abstracts proof generation so a
//! ZK backend can be swapped in later.

pub mod merkle;
pub mod stub;
pub mod traits;

#[cfg(feature = "sp1")]
pub mod sp1;

#[cfg(feature = "sp1")]
pub mod distribution;

pub use merkle::{hash_pair, merkle_leaf, MerkleProof, MerkleTree};
pub use stub::StubProver;
pub use traits::{ProofOutput, Prover, ProverError};

#[cfg(feature = "sp1")]
pub use sp1::Sp1Prover;

#[cfg(feature = "sp1")]
pub use distribution::{DistributionProver, DistributionGroth16Proof};

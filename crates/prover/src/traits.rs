//! Prover trait for pluggable proof backends.
//!
//! The stub prover builds a Merkle tree and returns the root.
//! A future ZK prover would generate a SNARK/STARK proof.

use tunnelcraft_core::ForwardReceipt;

/// Output of a proof generation.
#[derive(Debug, Clone)]
pub struct ProofOutput {
    /// New Merkle root after incorporating this batch.
    pub new_root: [u8; 32],
    /// Proof bytes (empty for stub, SNARK bytes for ZK).
    pub proof: Vec<u8>,
}

/// Errors from proof generation.
#[derive(Debug, thiserror::Error)]
pub enum ProverError {
    #[error("Empty batch")]
    EmptyBatch,

    #[error("Proof generation failed: {0}")]
    ProofFailed(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),
}

/// Pluggable prover trait.
///
/// Implementations generate a proof over a batch of `ForwardReceipt`s.
/// The stub prover hashes receipts into a Merkle tree. A ZK prover
/// would generate a cryptographic proof that the receipts are valid.
pub trait Prover: Send + Sync {
    /// Generate a proof over a batch of receipts.
    fn prove(&self, batch: &[ForwardReceipt]) -> Result<ProofOutput, ProverError>;

    /// Verify a proof against a root and batch size.
    fn verify(&self, root: &[u8; 32], proof: &[u8], batch_size: u64) -> Result<bool, ProverError>;
}

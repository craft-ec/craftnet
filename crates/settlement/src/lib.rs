//! TunnelCraft Settlement
//!
//! Solana client for on-chain settlement and credit management.
//!
//! ## Settlement Flow
//!
//! 1. **Purchase Credits**: User buys credits with `credit_hash = hash(credit_secret)`
//! 2. **Settle Request**: Exit node submits request settlement with `credit_secret`,
//!    consumes credit, awards points to all nodes in request chains (User → Relays → Exit),
//!    status is directly COMPLETE
//! 3. **Settle Response Shard**: Last relay for each response shard submits independently,
//!    awards points to all nodes in the response chain (Exit → Relays → User)
//! 4. **Claim Work**: Relays claim points from completed requests
//! 5. **Withdraw**: Nodes withdraw epoch rewards

mod client;
mod types;

pub use client::{SettlementClient, SettlementConfig, SettlementMode};
pub use types::*;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SettlementError {
    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Insufficient credits")]
    InsufficientCredits,

    #[error("Request not found: {0}")]
    RequestNotFound(String),

    #[error("Invalid credit secret")]
    InvalidCreditSecret,

    #[error("Destination mismatch: expected {expected}, got {actual}")]
    DestinationMismatch { expected: String, actual: String },

    #[error("Already settled")]
    AlreadySettled,

    #[error("Not authorized")]
    NotAuthorized,

    #[error("Epoch not complete")]
    EpochNotComplete,

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type Result<T> = std::result::Result<T, SettlementError>;

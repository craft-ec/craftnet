//! CraftNet Exit Node
//!
//! Exit node logic: HTTP fetch, TCP tunnel, and onion response creation.
//!
//! ## Responsibilities
//!
//! 1. Decrypt routing_tag to get assembly_id
//! 2. Collect and group shards by assembly_id
//! 3. Reconstruct via erasure coding and decrypt ExitPayload
//! 4. Execute HTTP request or open TCP tunnel
//! 5. Create onion-routed response shards via LeaseSet

mod handler;
mod request;
mod response;
mod tunnel_handler;

pub use handler::{ExitHandler, ExitConfig};
pub use request::HttpRequest;
pub use response::HttpResponse;
pub use tunnel_handler::TunnelHandler;

use thiserror::Error;
use craftnet_erasure::ErasureError;

#[derive(Error, Debug)]
pub enum ExitError {
    #[error("Insufficient shards: have {have}, need {need}")]
    InsufficientShards { have: usize, need: usize },

    #[error("Erasure decode failed: {0}")]
    ErasureDecodeError(String),

    #[error("Erasure error: {0}")]
    Erasure(#[from] ErasureError),

    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Invalid request format: {0}")]
    InvalidRequest(String),

    #[error("Settlement failed: {0}")]
    SettlementError(String),

    #[error("Request timeout")]
    Timeout,

    #[error("Blocked destination: {0}")]
    BlockedDestination(String),

    #[error("Tunnel connect failed: {0}")]
    TunnelConnectFailed(String),

    #[error("Tunnel I/O error: {0}")]
    TunnelIoError(String),

    #[error("Response too large: exceeds {0} byte limit")]
    ResponseTooLarge(usize),

    #[error("Rate limited: {0}")]
    RateLimited(String),
}

pub type Result<T> = std::result::Result<T, ExitError>;

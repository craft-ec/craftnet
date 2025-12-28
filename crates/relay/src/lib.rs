//! TunnelCraft Relay
//!
//! Relay logic and destination verification.
//! Relays cache `request_id → user_pubkey` and verify that response destinations match.
//!
//! ## Security Critical
//!
//! The destination verification is the core trustless mechanism that prevents
//! exit nodes from redirecting responses to colluding parties.

mod cache;
mod handler;

pub use cache::RequestCache;
pub use handler::{RelayHandler, RelayConfig, RelayError};

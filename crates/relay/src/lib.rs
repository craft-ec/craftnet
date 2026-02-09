//! TunnelCraft Relay
//!
//! Onion relay logic — peels one encrypted layer per hop to learn the next peer.
//! No plaintext routing metadata is visible. Gateway mode delivers shards to
//! registered clients via tunnel_id.

mod handler;

pub use handler::{RelayHandler, RelayConfig, RelayError};

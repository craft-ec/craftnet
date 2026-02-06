//! TunnelCraft Daemon
//!
//! Background service with IPC server for desktop/mobile frontends.
//!
//! ## Components
//!
//! - **DaemonService**: Client SDK wrapper with IPC interface
//! - **NodeService**: Relay/exit node runner (shared across all apps)
//! - **IpcServer**: JSON-RPC 2.0 over Unix sockets (macOS/Linux) or Named Pipes (Windows)
//!
//! ## IPC Methods
//!
//! - `connect` - Connect to VPN with optional hop count
//! - `disconnect` - Disconnect from VPN
//! - `status` - Get current connection status
//! - `purchase_credits` - Purchase credits on-chain
//! - `get_credits` - Get current credit balance
//!
//! ## Platform-Specific IPC
//!
//! - **macOS/Linux**: Unix domain sockets (`/tmp/tunnelcraft.sock`)
//! - **Windows**: Named pipes (`\\.\pipe\tunnelcraft`)

mod ipc;
mod node;
mod service;
mod windows_pipe;

pub use ipc::{IpcServer, IpcConfig, IpcHandler};
pub use node::{NodeService, NodeConfig, NodeType, NodeStats};
pub use service::{DaemonService, DaemonState};
pub use windows_pipe::{WindowsPipeServer, WindowsPipeConfig};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DaemonError {
    #[error("IPC error: {0}")]
    IpcError(String),

    #[error("Client error: {0}")]
    ClientError(#[from] tunnelcraft_client::ClientError),

    #[error("SDK error: {0}")]
    SdkError(String),

    #[error("Already running")]
    AlreadyRunning,

    #[error("Not running")]
    NotRunning,

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DaemonError>;

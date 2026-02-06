//! TunnelCraft Keystore
//!
//! Shared keypair management and path utilities for all TunnelCraft apps.
//!
//! ## Features
//!
//! - libp2p Ed25519 keypair loading/generation
//! - TunnelCraft signing keypair management
//! - Cross-platform path expansion (~, environment variables)
//! - Secure key storage utilities

mod keypair;
mod paths;

pub use keypair::{
    load_or_generate_libp2p_keypair,
    load_or_generate_signing_keypair,
    load_or_generate_keypair,
    default_key_path,
    save_keypair_bytes,
    KeystoreError,
};
pub use paths::{expand_path, default_keystore_dir, default_config_dir};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Keystore error: {0}")]
    Keystore(#[from] KeystoreError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

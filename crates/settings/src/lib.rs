//! TunnelCraft Settings
//!
//! Application configuration management for all TunnelCraft apps.
//!
//! ## Features
//!
//! - Network settings (hops, bootstrap peers)
//! - Node configuration (relay, exit, full)
//! - Cross-platform config file storage
//! - JSON serialization
//!
//! ## Usage
//!
//! ```no_run
//! use tunnelcraft_settings::{Settings, NetworkSettings};
//!
//! // Load or create default settings
//! let settings = Settings::load_or_default()?;
//!
//! // Modify settings
//! let mut settings = settings;
//! settings.network.default_hops = 3;
//!
//! // Save settings
//! settings.save()?;
//! # Ok::<(), tunnelcraft_settings::SettingsError>(())
//! ```

mod config;

pub use config::{
    Settings, NetworkSettings, NodeSettings, UiSettings,
    HopMode, NodeMode,
};

use std::path::PathBuf;

use thiserror::Error;
use tunnelcraft_keystore::default_config_dir;

#[derive(Error, Debug)]
pub enum SettingsError {
    #[error("Failed to read settings: {0}")]
    ReadError(std::io::Error),

    #[error("Failed to write settings: {0}")]
    WriteError(std::io::Error),

    #[error("Failed to parse settings: {0}")]
    ParseError(serde_json::Error),

    #[error("Failed to create config directory: {0}")]
    CreateDirError(std::io::Error),
}

pub type Result<T> = std::result::Result<T, SettingsError>;

/// Get the default settings file path
pub fn default_settings_path() -> PathBuf {
    default_config_dir().join("settings.json")
}

//! Configuration types

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{default_settings_path, Result, SettingsError};

/// Main settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Network settings
    #[serde(default)]
    pub network: NetworkSettings,

    /// Node settings (for running as relay/exit)
    #[serde(default)]
    pub node: NodeSettings,

    /// UI settings
    #[serde(default)]
    pub ui: UiSettings,

    /// Custom settings file path (not serialized)
    #[serde(skip)]
    config_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            network: NetworkSettings::default(),
            node: NodeSettings::default(),
            ui: UiSettings::default(),
            config_path: None,
        }
    }
}

impl Settings {
    /// Load settings from the default path, or create defaults
    pub fn load_or_default() -> Result<Self> {
        Self::load_from(&default_settings_path())
    }

    /// Load settings from a specific path, or create defaults
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path).map_err(SettingsError::ReadError)?;
            let mut settings: Settings =
                serde_json::from_str(&content).map_err(SettingsError::ParseError)?;
            settings.config_path = Some(path.clone());
            info!("Loaded settings from {:?}", path);
            Ok(settings)
        } else {
            let mut settings = Self::default();
            settings.config_path = Some(path.clone());
            Ok(settings)
        }
    }

    /// Save settings to the configured path
    pub fn save(&self) -> Result<()> {
        let path = self.config_path.clone().unwrap_or_else(default_settings_path);
        self.save_to(&path)
    }

    /// Save settings to a specific path
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(SettingsError::CreateDirError)?;
            }
        }

        let content = serde_json::to_string_pretty(self).map_err(SettingsError::ParseError)?;
        std::fs::write(path, content).map_err(SettingsError::WriteError)?;
        info!("Saved settings to {:?}", path);
        Ok(())
    }
}

/// Network settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    /// Default number of hops for new connections
    #[serde(default = "default_hops")]
    pub default_hops: u8,

    /// Default hop mode
    #[serde(default)]
    pub hop_mode: HopMode,

    /// Bootstrap peers (format: "peer_id@multiaddr")
    #[serde(default)]
    pub bootstrap_peers: Vec<String>,

    /// Auto-connect on startup
    #[serde(default)]
    pub auto_connect: bool,
}

fn default_hops() -> u8 {
    2
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            default_hops: default_hops(),
            hop_mode: HopMode::default(),
            bootstrap_peers: Vec::new(),
            auto_connect: false,
        }
    }
}

/// Hop mode for connections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HopMode {
    /// Direct connection (0 hops) - no privacy
    Direct,
    /// Light mode (1 hop) - basic privacy
    Light,
    /// Standard mode (2 hops) - recommended
    #[default]
    Standard,
    /// Paranoid mode (3+ hops) - maximum privacy
    Paranoid,
}

impl HopMode {
    /// Get the number of hops for this mode
    pub fn hops(&self) -> u8 {
        match self {
            Self::Direct => 0,
            Self::Light => 1,
            Self::Standard => 2,
            Self::Paranoid => 3,
        }
    }

    /// Create a hop mode from a hop count
    pub fn from_hops(hops: u8) -> Self {
        match hops {
            0 => Self::Direct,
            1 => Self::Light,
            2 => Self::Standard,
            _ => Self::Paranoid,
        }
    }
}

/// Node settings (for running as relay/exit)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSettings {
    /// Node mode
    #[serde(default)]
    pub mode: NodeMode,

    /// Listen address
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// Allow being last hop (relay only)
    #[serde(default = "default_true")]
    pub allow_last_hop: bool,

    /// HTTP request timeout in seconds (exit only)
    #[serde(default = "default_timeout")]
    pub request_timeout_secs: u64,

    /// Keyfile path
    #[serde(default)]
    pub keyfile: Option<String>,
}

fn default_listen_addr() -> String {
    "/ip4/0.0.0.0/tcp/9000".to_string()
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

impl Default for NodeSettings {
    fn default() -> Self {
        Self {
            mode: NodeMode::default(),
            listen_addr: default_listen_addr(),
            allow_last_hop: true,
            request_timeout_secs: default_timeout(),
            keyfile: None,
        }
    }
}

/// Node operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NodeMode {
    /// Disabled - don't run as a node
    #[default]
    Disabled,
    /// Relay only - forward traffic
    Relay,
    /// Exit only - fetch from internet
    Exit,
    /// Full node - relay and exit
    Full,
}

/// UI settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    /// Show notifications
    #[serde(default = "default_true")]
    pub notifications: bool,

    /// Start minimized
    #[serde(default)]
    pub start_minimized: bool,

    /// Theme (light/dark/system)
    #[serde(default)]
    pub theme: Theme,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            notifications: true,
            start_minimized: false,
            theme: Theme::default(),
        }
    }
}

/// UI theme
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Light theme
    Light,
    /// Dark theme
    Dark,
    /// Follow system preference
    #[default]
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.network.default_hops, 2);
        assert_eq!(settings.network.hop_mode, HopMode::Standard);
        assert!(settings.network.bootstrap_peers.is_empty());
    }

    #[test]
    fn test_hop_mode_conversion() {
        assert_eq!(HopMode::Direct.hops(), 0);
        assert_eq!(HopMode::Light.hops(), 1);
        assert_eq!(HopMode::Standard.hops(), 2);
        assert_eq!(HopMode::Paranoid.hops(), 3);

        assert_eq!(HopMode::from_hops(0), HopMode::Direct);
        assert_eq!(HopMode::from_hops(1), HopMode::Light);
        assert_eq!(HopMode::from_hops(2), HopMode::Standard);
        assert_eq!(HopMode::from_hops(3), HopMode::Paranoid);
        assert_eq!(HopMode::from_hops(10), HopMode::Paranoid);
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.network.default_hops, settings.network.default_hops);
    }

    #[test]
    fn test_node_settings_default() {
        let node = NodeSettings::default();
        assert_eq!(node.mode, NodeMode::Disabled);
        assert!(node.allow_last_hop);
        assert_eq!(node.request_timeout_secs, 30);
    }

    #[test]
    fn test_ui_settings_default() {
        let ui = UiSettings::default();
        assert!(ui.notifications);
        assert!(!ui.start_minimized);
        assert_eq!(ui.theme, Theme::System);
    }
}

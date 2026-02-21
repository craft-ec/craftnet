//! Configuration types

use serde::{Deserialize, Serialize};

/// Main settings structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CraftNetConfig {
    /// Network settings
    #[serde(default)]
    pub network: NetworkSettings,

    /// Node settings (for running as relay/exit)
    #[serde(default)]
    pub node: NodeSettings,

    /// UI settings
    #[serde(default)]
    pub ui: UiSettings,
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

/// Hop mode for connections (number of relay hops)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HopMode {
    /// 0 hops - direct to exit (free tier, exit sees client IP)
    Direct,
    /// 1 hop (gateway only) - fastest
    Single,
    /// 2 hops - basic privacy
    Double,
    /// 3 hops - good privacy (recommended)
    #[default]
    Triple,
    /// 4 hops - maximum privacy
    Quad,
}

impl HopMode {
    /// Get the number of hops for this mode
    pub fn hops(&self) -> u8 {
        match self {
            Self::Direct => 0,
            Self::Single => 1,
            Self::Double => 2,
            Self::Triple => 3,
            Self::Quad => 4,
        }
    }

    /// Create a hop mode from a hop count
    pub fn from_hops(hops: u8) -> Self {
        match hops {
            0 => Self::Direct,
            1 => Self::Single,
            2 => Self::Double,
            3 => Self::Triple,
            _ => Self::Quad,
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
        let settings = CraftNetConfig::default();
        assert_eq!(settings.network.default_hops, 2);
        assert_eq!(settings.network.hop_mode, HopMode::Triple);
        assert!(settings.network.bootstrap_peers.is_empty());
    }

    #[test]
    fn test_hop_mode_conversion() {
        assert_eq!(HopMode::Direct.hops(), 0);
        assert_eq!(HopMode::Single.hops(), 1);
        assert_eq!(HopMode::Double.hops(), 2);
        assert_eq!(HopMode::Triple.hops(), 3);
        assert_eq!(HopMode::Quad.hops(), 4);

        assert_eq!(HopMode::from_hops(0), HopMode::Direct);
        assert_eq!(HopMode::from_hops(1), HopMode::Single);
        assert_eq!(HopMode::from_hops(2), HopMode::Double);
        assert_eq!(HopMode::from_hops(3), HopMode::Triple);
        assert_eq!(HopMode::from_hops(10), HopMode::Quad);
    }

    #[test]
    fn test_settings_serialization() {
        let settings = CraftNetConfig::default();
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: CraftNetConfig = serde_json::from_str(&json).unwrap();
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

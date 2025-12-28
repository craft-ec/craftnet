//! Feature flags for controlling application capabilities
//!
//! Features are organized by layer and can be enabled/disabled at runtime.

use std::collections::HashSet;

/// All application features
#[derive(Debug, Clone, Default)]
pub struct Features {
    /// Backend layer features
    pub backend: HashSet<BackendFeatures>,
    /// Integration layer features
    pub integration: HashSet<IntegrationFeatures>,
    /// Frontend layer features
    pub frontend: HashSet<FrontendFeatures>,
}

impl Features {
    /// Create features for a client application
    pub fn client() -> Self {
        Self {
            backend: [
                BackendFeatures::Network,
                BackendFeatures::Crypto,
                BackendFeatures::Erasure,
            ]
            .into_iter()
            .collect(),
            integration: [IntegrationFeatures::Sdk, IntegrationFeatures::Ipc]
                .into_iter()
                .collect(),
            frontend: [FrontendFeatures::Cli].into_iter().collect(),
        }
    }

    /// Create features for a node operator
    pub fn node() -> Self {
        Self {
            backend: [
                BackendFeatures::Network,
                BackendFeatures::Crypto,
                BackendFeatures::Erasure,
                BackendFeatures::Relay,
                BackendFeatures::Exit,
                BackendFeatures::Settlement,
            ]
            .into_iter()
            .collect(),
            integration: [IntegrationFeatures::NodeService]
                .into_iter()
                .collect(),
            frontend: HashSet::new(),
        }
    }

    /// Create features for the daemon service
    pub fn daemon() -> Self {
        Self {
            backend: [
                BackendFeatures::Network,
                BackendFeatures::Crypto,
                BackendFeatures::Erasure,
            ]
            .into_iter()
            .collect(),
            integration: [
                IntegrationFeatures::Sdk,
                IntegrationFeatures::Ipc,
                IntegrationFeatures::Daemon,
            ]
            .into_iter()
            .collect(),
            frontend: HashSet::new(),
        }
    }

    /// Create features for desktop application
    pub fn desktop() -> Self {
        let mut features = Self::client();
        features.frontend.insert(FrontendFeatures::Desktop);
        features.frontend.insert(FrontendFeatures::Notifications);
        features.frontend.insert(FrontendFeatures::SystemTray);
        features
    }

    /// Create features for mobile application
    pub fn mobile() -> Self {
        let mut features = Self::client();
        features.frontend.remove(&FrontendFeatures::Cli);
        features.frontend.insert(FrontendFeatures::Mobile);
        features.frontend.insert(FrontendFeatures::Notifications);
        features.integration.insert(IntegrationFeatures::Ffi);
        features
    }

    /// Create all features enabled
    pub fn all() -> Self {
        Self {
            backend: BackendFeatures::all(),
            integration: IntegrationFeatures::all(),
            frontend: FrontendFeatures::all(),
        }
    }

    /// Create minimal features (backend only)
    pub fn minimal() -> Self {
        Self {
            backend: [BackendFeatures::Crypto].into_iter().collect(),
            integration: HashSet::new(),
            frontend: HashSet::new(),
        }
    }

    /// Enable a backend feature
    pub fn enable_backend(&mut self, feature: BackendFeatures) -> &mut Self {
        self.backend.insert(feature);
        self
    }

    /// Enable an integration feature
    pub fn enable_integration(&mut self, feature: IntegrationFeatures) -> &mut Self {
        self.integration.insert(feature);
        self
    }

    /// Enable a frontend feature
    pub fn enable_frontend(&mut self, feature: FrontendFeatures) -> &mut Self {
        self.frontend.insert(feature);
        self
    }

    /// Disable a backend feature
    pub fn disable_backend(&mut self, feature: BackendFeatures) -> &mut Self {
        self.backend.remove(&feature);
        self
    }

    /// Disable an integration feature
    pub fn disable_integration(&mut self, feature: IntegrationFeatures) -> &mut Self {
        self.integration.remove(&feature);
        self
    }

    /// Disable a frontend feature
    pub fn disable_frontend(&mut self, feature: FrontendFeatures) -> &mut Self {
        self.frontend.remove(&feature);
        self
    }
}

/// Backend layer features
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendFeatures {
    /// P2P networking (libp2p)
    Network,
    /// Cryptographic operations
    Crypto,
    /// Erasure coding (Reed-Solomon)
    Erasure,
    /// Relay node functionality
    Relay,
    /// Exit node functionality
    Exit,
    /// On-chain settlement (Solana)
    Settlement,
    /// DHT peer discovery
    Dht,
    /// mDNS local discovery
    Mdns,
    /// NAT traversal (relay, DCUtR)
    NatTraversal,
}

impl BackendFeatures {
    /// Get all backend features
    pub fn all() -> HashSet<Self> {
        [
            Self::Network,
            Self::Crypto,
            Self::Erasure,
            Self::Relay,
            Self::Exit,
            Self::Settlement,
            Self::Dht,
            Self::Mdns,
            Self::NatTraversal,
        ]
        .into_iter()
        .collect()
    }
}

/// Integration layer features
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegrationFeatures {
    /// Client SDK
    Sdk,
    /// IPC server/client
    Ipc,
    /// Background daemon
    Daemon,
    /// Node service
    NodeService,
    /// FFI bindings (mobile)
    Ffi,
}

impl IntegrationFeatures {
    /// Get all integration features
    pub fn all() -> HashSet<Self> {
        [
            Self::Sdk,
            Self::Ipc,
            Self::Daemon,
            Self::NodeService,
            Self::Ffi,
        ]
        .into_iter()
        .collect()
    }
}

/// Frontend layer features
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontendFeatures {
    /// Command-line interface
    Cli,
    /// Desktop UI (Electron)
    Desktop,
    /// Mobile UI (React Native)
    Mobile,
    /// System notifications
    Notifications,
    /// System tray icon
    SystemTray,
    /// Auto-start on boot
    AutoStart,
    /// Dark mode support
    DarkMode,
}

impl FrontendFeatures {
    /// Get all frontend features
    pub fn all() -> HashSet<Self> {
        [
            Self::Cli,
            Self::Desktop,
            Self::Mobile,
            Self::Notifications,
            Self::SystemTray,
            Self::AutoStart,
            Self::DarkMode,
        ]
        .into_iter()
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_features() {
        let features = Features::client();
        assert!(features.backend.contains(&BackendFeatures::Network));
        assert!(features.backend.contains(&BackendFeatures::Crypto));
        assert!(features.integration.contains(&IntegrationFeatures::Sdk));
        assert!(features.frontend.contains(&FrontendFeatures::Cli));
    }

    #[test]
    fn test_node_features() {
        let features = Features::node();
        assert!(features.backend.contains(&BackendFeatures::Relay));
        assert!(features.backend.contains(&BackendFeatures::Exit));
        assert!(features.backend.contains(&BackendFeatures::Settlement));
        assert!(features.frontend.is_empty());
    }

    #[test]
    fn test_daemon_features() {
        let features = Features::daemon();
        assert!(features.integration.contains(&IntegrationFeatures::Ipc));
        assert!(features.integration.contains(&IntegrationFeatures::Daemon));
    }

    #[test]
    fn test_desktop_features() {
        let features = Features::desktop();
        assert!(features.frontend.contains(&FrontendFeatures::Desktop));
        assert!(features.frontend.contains(&FrontendFeatures::Notifications));
        assert!(features.frontend.contains(&FrontendFeatures::SystemTray));
    }

    #[test]
    fn test_mobile_features() {
        let features = Features::mobile();
        assert!(features.frontend.contains(&FrontendFeatures::Mobile));
        assert!(features.integration.contains(&IntegrationFeatures::Ffi));
        assert!(!features.frontend.contains(&FrontendFeatures::Cli));
    }

    #[test]
    fn test_feature_modification() {
        let mut features = Features::minimal();
        assert!(!features.backend.contains(&BackendFeatures::Network));

        features.enable_backend(BackendFeatures::Network);
        assert!(features.backend.contains(&BackendFeatures::Network));

        features.disable_backend(BackendFeatures::Network);
        assert!(!features.backend.contains(&BackendFeatures::Network));
    }
}

//! TunnelCraft App Framework
//!
//! Standard initialization and feature control for all TunnelCraft applications.
//!
//! ## Architecture Layers
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      FRONTEND LAYER                         │
//! │  Desktop (Electron) | Mobile (React Native) | CLI           │
//! │  - UI/UX components                                         │
//! │  - User interaction                                         │
//! │  - Platform-specific features                               │
//! ├─────────────────────────────────────────────────────────────┤
//! │                    INTEGRATION LAYER                        │
//! │  Daemon Service | IPC Server | FFI Bindings                 │
//! │  - Process management                                       │
//! │  - Inter-process communication                              │
//! │  - Platform bridges                                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │                      BACKEND LAYER                          │
//! │  Network | Relay | Exit | Settlement | Crypto               │
//! │  - P2P networking (libp2p)                                  │
//! │  - Shard processing                                         │
//! │  - Cryptographic operations                                 │
//! │  - On-chain settlement                                      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use tunnelcraft_app::{App, AppType, Features};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize CLI application with default features
//!     let app = App::builder()
//!         .name("tunnelcraft")
//!         .app_type(AppType::Cli)
//!         .verbose(true)
//!         .build()?;
//!
//!     // Access initialized components
//!     let settings = app.settings();
//!
//!     Ok(())
//! }
//! ```

mod builder;
mod features;
mod layers;
mod matrix;

pub use builder::AppBuilder;
pub use features::{Features, BackendFeatures, IntegrationFeatures, FrontendFeatures};
pub use layers::Layer;
pub use matrix::{ImplementationMatrix, Feature, Platform, Status, LayerMatrix};

use std::sync::Arc;

use thiserror::Error;
use tracing::info;

use tunnelcraft_settings::Settings;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Initialization failed: {0}")]
    InitFailed(String),

    #[error("Settings error: {0}")]
    Settings(#[from] tunnelcraft_settings::SettingsError),

    #[error("Feature not enabled: {0}")]
    FeatureNotEnabled(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

/// Application type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppType {
    /// Command-line interface
    Cli,
    /// Desktop application (Electron)
    Desktop,
    /// Mobile application (React Native)
    Mobile,
    /// Background daemon service
    Daemon,
    /// Network node (relay/exit)
    Node,
}

impl AppType {
    /// Get the default layer for this app type
    pub fn default_layer(&self) -> Layer {
        match self {
            Self::Cli => Layer::Frontend,
            Self::Desktop => Layer::Frontend,
            Self::Mobile => Layer::Frontend,
            Self::Daemon => Layer::Integration,
            Self::Node => Layer::Backend,
        }
    }

    /// Get the app name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Cli => "tunnelcraft-cli",
            Self::Desktop => "tunnelcraft-desktop",
            Self::Mobile => "tunnelcraft-mobile",
            Self::Daemon => "tunnelcraft-daemon",
            Self::Node => "tunnelcraft-node",
        }
    }
}

/// Initialized TunnelCraft application
pub struct App {
    name: String,
    version: String,
    app_type: AppType,
    features: Features,
    settings: Arc<Settings>,
}

impl App {
    /// Create a new app builder
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    /// Get the application name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the application version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get the application type
    pub fn app_type(&self) -> AppType {
        self.app_type
    }

    /// Get enabled features
    pub fn features(&self) -> &Features {
        &self.features
    }

    /// Get application settings
    pub fn settings(&self) -> Arc<Settings> {
        self.settings.clone()
    }

    /// Check if a backend feature is enabled
    pub fn has_backend_feature(&self, feature: BackendFeatures) -> bool {
        self.features.backend.contains(&feature)
    }

    /// Check if an integration feature is enabled
    pub fn has_integration_feature(&self, feature: IntegrationFeatures) -> bool {
        self.features.integration.contains(&feature)
    }

    /// Check if a frontend feature is enabled
    pub fn has_frontend_feature(&self, feature: FrontendFeatures) -> bool {
        self.features.frontend.contains(&feature)
    }

    /// Log startup banner
    pub fn log_startup(&self) {
        info!("╔════════════════════════════════════════╗");
        info!("║         TunnelCraft {:^10}         ║", self.version);
        info!("╠════════════════════════════════════════╣");
        info!("║  App: {:<32} ║", self.name);
        info!("║  Type: {:<31} ║", format!("{:?}", self.app_type));
        info!("║  Layer: {:<30} ║", format!("{:?}", self.app_type.default_layer()));
        info!("╚════════════════════════════════════════╝");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_type_layer() {
        assert_eq!(AppType::Cli.default_layer(), Layer::Frontend);
        assert_eq!(AppType::Desktop.default_layer(), Layer::Frontend);
        assert_eq!(AppType::Mobile.default_layer(), Layer::Frontend);
        assert_eq!(AppType::Daemon.default_layer(), Layer::Integration);
        assert_eq!(AppType::Node.default_layer(), Layer::Backend);
    }

    #[test]
    fn test_app_type_name() {
        assert_eq!(AppType::Cli.name(), "tunnelcraft-cli");
        assert_eq!(AppType::Daemon.name(), "tunnelcraft-daemon");
    }
}

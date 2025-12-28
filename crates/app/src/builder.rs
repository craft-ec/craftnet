//! App builder for fluent initialization

use std::path::PathBuf;
use std::sync::Arc;

use tunnelcraft_logging::{try_init as try_init_logging, LogLevel};
use tunnelcraft_settings::Settings;

use crate::{App, AppType, Features, Result};

/// Builder for creating TunnelCraft applications
pub struct AppBuilder {
    name: Option<String>,
    version: Option<String>,
    app_type: Option<AppType>,
    features: Option<Features>,
    verbose: bool,
    log_level: Option<LogLevel>,
    config_path: Option<PathBuf>,
    skip_logging: bool,
    skip_settings: bool,
    skip_banner: bool,
}

impl AppBuilder {
    /// Create a new app builder
    pub fn new() -> Self {
        Self {
            name: None,
            version: None,
            app_type: None,
            features: None,
            verbose: false,
            log_level: None,
            config_path: None,
            skip_logging: false,
            skip_settings: false,
            skip_banner: false,
        }
    }

    /// Set the application name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the application version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the application type
    pub fn app_type(mut self, app_type: AppType) -> Self {
        self.app_type = Some(app_type);
        self
    }

    /// Set custom features
    pub fn features(mut self, features: Features) -> Self {
        self.features = Some(features);
        self
    }

    /// Enable verbose logging (debug level)
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Set explicit log level
    pub fn log_level(mut self, level: LogLevel) -> Self {
        self.log_level = Some(level);
        self
    }

    /// Set custom config path
    pub fn config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Skip logging initialization (useful for tests)
    pub fn skip_logging(mut self) -> Self {
        self.skip_logging = true;
        self
    }

    /// Skip settings loading
    pub fn skip_settings(mut self) -> Self {
        self.skip_settings = true;
        self
    }

    /// Skip startup banner
    pub fn skip_banner(mut self) -> Self {
        self.skip_banner = true;
        self
    }

    /// Build the application
    pub fn build(self) -> Result<App> {
        // Determine app type and defaults
        let app_type = self.app_type.unwrap_or(AppType::Cli);
        let name = self.name.unwrap_or_else(|| app_type.name().to_string());
        let version = self.version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

        // Determine features based on app type if not provided
        let features = self.features.unwrap_or_else(|| match app_type {
            AppType::Cli => Features::client(),
            AppType::Desktop => Features::desktop(),
            AppType::Mobile => Features::mobile(),
            AppType::Daemon => Features::daemon(),
            AppType::Node => Features::node(),
        });

        // Initialize logging
        if !self.skip_logging {
            let level = self.log_level.unwrap_or_else(|| {
                if self.verbose {
                    LogLevel::Debug
                } else {
                    LogLevel::Info
                }
            });

            // Try to initialize, ignore if already initialized
            let _ = try_init_logging(level);
        }

        // Load settings
        let settings = if self.skip_settings {
            Settings::default()
        } else if let Some(path) = self.config_path {
            Settings::load_from(&path)?
        } else {
            Settings::load_or_default()?
        };

        let app = App {
            name,
            version,
            app_type,
            features,
            settings: Arc::new(settings),
        };

        // Log startup banner
        if !self.skip_banner && !self.skip_logging {
            app.log_startup();
        }

        Ok(app)
    }

    /// Build a CLI application with defaults
    pub fn cli() -> Result<App> {
        Self::new().app_type(AppType::Cli).build()
    }

    /// Build a daemon application with defaults
    pub fn daemon() -> Result<App> {
        Self::new().app_type(AppType::Daemon).build()
    }

    /// Build a node application with defaults
    pub fn node() -> Result<App> {
        Self::new().app_type(AppType::Node).build()
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let app = AppBuilder::new()
            .skip_logging()
            .skip_settings()
            .skip_banner()
            .build()
            .unwrap();

        assert_eq!(app.app_type(), AppType::Cli);
        assert!(!app.name().is_empty());
    }

    #[test]
    fn test_builder_custom_name() {
        let app = AppBuilder::new()
            .name("my-app")
            .version("1.0.0")
            .skip_logging()
            .skip_settings()
            .skip_banner()
            .build()
            .unwrap();

        assert_eq!(app.name(), "my-app");
        assert_eq!(app.version(), "1.0.0");
    }

    #[test]
    fn test_builder_app_type() {
        let app = AppBuilder::new()
            .app_type(AppType::Daemon)
            .skip_logging()
            .skip_settings()
            .skip_banner()
            .build()
            .unwrap();

        assert_eq!(app.app_type(), AppType::Daemon);
        assert!(app.has_integration_feature(crate::IntegrationFeatures::Daemon));
    }

    #[test]
    fn test_builder_custom_features() {
        let features = Features::minimal();
        let app = AppBuilder::new()
            .features(features)
            .skip_logging()
            .skip_settings()
            .skip_banner()
            .build()
            .unwrap();

        assert!(app.has_backend_feature(crate::BackendFeatures::Crypto));
        assert!(!app.has_backend_feature(crate::BackendFeatures::Network));
    }

    #[test]
    fn test_quick_builders() {
        // These should not fail (skip everything for tests)
        let _ = AppBuilder::new()
            .app_type(AppType::Cli)
            .skip_logging()
            .skip_settings()
            .skip_banner()
            .build()
            .unwrap();
    }
}

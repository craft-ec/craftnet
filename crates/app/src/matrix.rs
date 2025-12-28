//! Feature Implementation Matrix
//!
//! Tracks feature implementation status across all platforms.
//!
//! ## Organization
//!
//! ```text
//! Layer → Feature → Platform Status
//!
//! ┌─────────────────┬───────┬─────────┬────────┬────────┬──────┐
//! │ Feature         │  CLI  │ Desktop │ Mobile │ Daemon │ Node │
//! ├─────────────────┼───────┼─────────┼────────┼────────┼──────┤
//! │ BACKEND         │       │         │        │        │      │
//! │  Network        │  ✓    │  ✓ ipc  │  ✗ ffi │   ✓    │  ✓   │
//! │  Crypto         │  ✓    │  ✓ ipc  │  ✗ ffi │   ✓    │  ✓   │
//! │  Relay          │  ✓    │   n/a   │  n/a   │  n/a   │  ✓   │
//! │  Exit           │  ✓    │   n/a   │  n/a   │  n/a   │  ✓   │
//! │  Settlement     │  ✗    │   n/a   │  n/a   │   ✗    │  ✗   │
//! ├─────────────────┼───────┼─────────┼────────┼────────┼──────┤
//! │ INTEGRATION     │       │         │        │        │      │
//! │  SDK            │  ✓    │  ✓ ipc  │  ✗ ffi │   ✓    │ n/a  │
//! │  IPC            │  ✓    │   ✓     │   ✗    │   ✓    │ n/a  │
//! │  FFI            │ n/a   │  n/a    │   ✗    │  n/a   │ n/a  │
//! │  NodeService    │  ✓    │  n/a    │  n/a   │   ✓    │  ✓   │
//! ├─────────────────┼───────┼─────────┼────────┼────────┼──────┤
//! │ FRONTEND        │       │         │        │        │      │
//! │  UI             │  ✓    │   ✗     │   ✗    │  n/a   │ n/a  │
//! │  Notifications  │ n/a   │   ✗     │   ✗    │  n/a   │ n/a  │
//! │  SystemTray     │ n/a   │   ✗     │  n/a   │  n/a   │ n/a  │
//! └─────────────────┴───────┴─────────┴────────┴────────┴──────┘
//!
//! Legend: ✓ = implemented, ✗ = needs work, n/a = not applicable
//! ```

use std::collections::HashMap;

/// Target platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Command-line interface (macOS, Linux)
    Cli,
    /// Desktop application (Electron - macOS, Windows, Linux)
    Desktop,
    /// Mobile application (React Native - iOS, Android)
    Mobile,
    /// Background daemon service
    Daemon,
    /// Network node operator
    Node,
}

impl Platform {
    /// Get all platforms
    pub fn all() -> &'static [Platform] {
        &[
            Platform::Cli,
            Platform::Desktop,
            Platform::Mobile,
            Platform::Daemon,
            Platform::Node,
        ]
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            Platform::Cli => "CLI",
            Platform::Desktop => "Desktop",
            Platform::Mobile => "Mobile",
            Platform::Daemon => "Daemon",
            Platform::Node => "Node",
        }
    }

    /// Get sub-platforms (OS/device specific)
    pub fn sub_platforms(&self) -> &'static [&'static str] {
        match self {
            Platform::Cli => &["macOS", "Linux"],
            Platform::Desktop => &["macOS", "Windows", "Linux"],
            Platform::Mobile => &["iOS", "Android"],
            Platform::Daemon => &["macOS", "Windows", "Linux"],
            Platform::Node => &["macOS", "Linux"],
        }
    }
}

/// Implementation status for a feature on a platform
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Fully implemented and working
    Implemented,
    /// Partially implemented
    Partial,
    /// Needs implementation
    NotImplemented,
    /// Not applicable to this platform
    NotApplicable,
    /// Implemented via another component (e.g., via IPC to daemon)
    ViaProxy(&'static str),
}

impl Status {
    /// Check if this needs work
    pub fn needs_work(&self) -> bool {
        matches!(self, Status::NotImplemented | Status::Partial)
    }

    /// Get display symbol
    pub fn symbol(&self) -> &'static str {
        match self {
            Status::Implemented => "✓",
            Status::Partial => "◐",
            Status::NotImplemented => "✗",
            Status::NotApplicable => "—",
            Status::ViaProxy(_) => "→",
        }
    }
}

/// A feature with its implementation status across platforms
#[derive(Debug, Clone)]
pub struct Feature {
    /// Feature identifier
    pub id: &'static str,
    /// Human-readable name
    pub name: &'static str,
    /// Description
    pub description: &'static str,
    /// Implementation status per platform
    pub status: HashMap<Platform, Status>,
    /// Dependencies on other features
    pub depends_on: Vec<&'static str>,
}

impl Feature {
    /// Create a new feature
    pub fn new(id: &'static str, name: &'static str, description: &'static str) -> Self {
        Self {
            id,
            name,
            description,
            status: HashMap::new(),
            depends_on: Vec::new(),
        }
    }

    /// Set status for a platform
    pub fn with_status(mut self, platform: Platform, status: Status) -> Self {
        self.status.insert(platform, status);
        self
    }

    /// Add dependency
    pub fn depends(mut self, feature_id: &'static str) -> Self {
        self.depends_on.push(feature_id);
        self
    }

    /// Get status for a platform
    pub fn get_status(&self, platform: Platform) -> Status {
        self.status.get(&platform).copied().unwrap_or(Status::NotApplicable)
    }

    /// Check if feature needs work on any platform
    pub fn has_gaps(&self) -> bool {
        self.status.values().any(|s| s.needs_work())
    }

    /// Get platforms that need work
    pub fn platforms_needing_work(&self) -> Vec<Platform> {
        self.status
            .iter()
            .filter(|(_, s)| s.needs_work())
            .map(|(p, _)| *p)
            .collect()
    }
}

/// Feature matrix for a layer
#[derive(Debug, Clone)]
pub struct LayerMatrix {
    /// Layer name
    pub name: &'static str,
    /// Features in this layer
    pub features: Vec<Feature>,
}

impl LayerMatrix {
    /// Create a new layer matrix
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            features: Vec::new(),
        }
    }

    /// Add a feature
    pub fn with_feature(mut self, feature: Feature) -> Self {
        self.features.push(feature);
        self
    }

    /// Get all features needing work
    pub fn features_with_gaps(&self) -> Vec<&Feature> {
        self.features.iter().filter(|f| f.has_gaps()).collect()
    }
}

/// Complete implementation matrix
#[derive(Debug, Clone)]
pub struct ImplementationMatrix {
    pub backend: LayerMatrix,
    pub integration: LayerMatrix,
    pub frontend: LayerMatrix,
}

impl ImplementationMatrix {
    /// Create the current implementation matrix
    pub fn current() -> Self {
        Self {
            backend: Self::backend_layer(),
            integration: Self::integration_layer(),
            frontend: Self::frontend_layer(),
        }
    }

    fn backend_layer() -> LayerMatrix {
        LayerMatrix::new("Backend")
            .with_feature(
                Feature::new("network", "P2P Network", "libp2p networking stack")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::ViaProxy("daemon"))
                    .with_status(Platform::Mobile, Status::NotImplemented)
                    .with_status(Platform::Daemon, Status::Implemented)
                    .with_status(Platform::Node, Status::Implemented),
            )
            .with_feature(
                Feature::new("crypto", "Cryptography", "Encryption and signatures")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::ViaProxy("daemon"))
                    .with_status(Platform::Mobile, Status::NotImplemented)
                    .with_status(Platform::Daemon, Status::Implemented)
                    .with_status(Platform::Node, Status::Implemented),
            )
            .with_feature(
                Feature::new("erasure", "Erasure Coding", "Reed-Solomon shard coding")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::ViaProxy("daemon"))
                    .with_status(Platform::Mobile, Status::NotImplemented)
                    .with_status(Platform::Daemon, Status::Implemented)
                    .with_status(Platform::Node, Status::Implemented),
            )
            .with_feature(
                Feature::new("relay", "Relay Node", "Shard forwarding")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::NotApplicable)
                    .with_status(Platform::Mobile, Status::NotApplicable)
                    .with_status(Platform::Daemon, Status::Implemented)
                    .with_status(Platform::Node, Status::Implemented),
            )
            .with_feature(
                Feature::new("exit", "Exit Node", "HTTP fetching")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::NotApplicable)
                    .with_status(Platform::Mobile, Status::NotApplicable)
                    .with_status(Platform::Daemon, Status::Implemented)
                    .with_status(Platform::Node, Status::Implemented),
            )
            .with_feature(
                Feature::new("settlement", "Settlement", "Solana on-chain settlement")
                    .with_status(Platform::Cli, Status::NotImplemented)
                    .with_status(Platform::Desktop, Status::NotApplicable)
                    .with_status(Platform::Mobile, Status::NotApplicable)
                    .with_status(Platform::Daemon, Status::NotImplemented)
                    .with_status(Platform::Node, Status::NotImplemented),
            )
    }

    fn integration_layer() -> LayerMatrix {
        LayerMatrix::new("Integration")
            .with_feature(
                Feature::new("sdk", "Client SDK", "High-level client interface")
                    .depends("network")
                    .depends("crypto")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::ViaProxy("ipc"))
                    .with_status(Platform::Mobile, Status::NotImplemented)
                    .with_status(Platform::Daemon, Status::Implemented)
                    .with_status(Platform::Node, Status::NotApplicable),
            )
            .with_feature(
                Feature::new("ipc", "IPC Communication", "JSON-RPC over sockets")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::Partial)
                    .with_status(Platform::Mobile, Status::NotImplemented)
                    .with_status(Platform::Daemon, Status::Implemented)
                    .with_status(Platform::Node, Status::NotApplicable),
            )
            .with_feature(
                Feature::new("ffi", "FFI Bindings", "uniffi mobile bindings")
                    .depends("sdk")
                    .with_status(Platform::Cli, Status::NotApplicable)
                    .with_status(Platform::Desktop, Status::NotApplicable)
                    .with_status(Platform::Mobile, Status::Partial) // UniFFI bindings done, Swift wrapper needed
                    .with_status(Platform::Daemon, Status::NotApplicable)
                    .with_status(Platform::Node, Status::NotApplicable),
            )
            .with_feature(
                Feature::new("node_service", "Node Service", "Shared relay/exit runner")
                    .depends("relay")
                    .depends("exit")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::NotApplicable)
                    .with_status(Platform::Mobile, Status::NotApplicable)
                    .with_status(Platform::Daemon, Status::Implemented)
                    .with_status(Platform::Node, Status::Implemented),
            )
    }

    fn frontend_layer() -> LayerMatrix {
        LayerMatrix::new("Frontend")
            .with_feature(
                Feature::new("cli_ui", "CLI Interface", "Command-line user interface")
                    .depends("sdk")
                    .with_status(Platform::Cli, Status::Implemented)
                    .with_status(Platform::Desktop, Status::NotApplicable)
                    .with_status(Platform::Mobile, Status::NotApplicable)
                    .with_status(Platform::Daemon, Status::NotApplicable)
                    .with_status(Platform::Node, Status::NotApplicable),
            )
            .with_feature(
                Feature::new("desktop_ui", "Desktop UI", "Electron application")
                    .depends("ipc")
                    .with_status(Platform::Cli, Status::NotApplicable)
                    .with_status(Platform::Desktop, Status::NotImplemented)
                    .with_status(Platform::Mobile, Status::NotApplicable)
                    .with_status(Platform::Daemon, Status::NotApplicable)
                    .with_status(Platform::Node, Status::NotApplicable),
            )
            .with_feature(
                Feature::new("mobile_ui", "Mobile UI", "iOS SwiftUI / React Native application")
                    .depends("ffi")
                    .with_status(Platform::Cli, Status::NotApplicable)
                    .with_status(Platform::Desktop, Status::NotApplicable)
                    .with_status(Platform::Mobile, Status::Partial) // iOS SwiftUI done, Android pending
                    .with_status(Platform::Daemon, Status::NotApplicable)
                    .with_status(Platform::Node, Status::NotApplicable),
            )
            .with_feature(
                Feature::new("notifications", "Notifications", "System notifications")
                    .with_status(Platform::Cli, Status::NotApplicable)
                    .with_status(Platform::Desktop, Status::NotImplemented)
                    .with_status(Platform::Mobile, Status::NotImplemented)
                    .with_status(Platform::Daemon, Status::NotApplicable)
                    .with_status(Platform::Node, Status::NotApplicable),
            )
            .with_feature(
                Feature::new("system_tray", "System Tray", "Tray icon and menu")
                    .with_status(Platform::Cli, Status::NotApplicable)
                    .with_status(Platform::Desktop, Status::NotImplemented)
                    .with_status(Platform::Mobile, Status::NotApplicable)
                    .with_status(Platform::Daemon, Status::NotApplicable)
                    .with_status(Platform::Node, Status::NotApplicable),
            )
    }

    /// Get all features with implementation gaps
    pub fn all_gaps(&self) -> Vec<(&'static str, &Feature)> {
        let mut gaps = Vec::new();

        for feature in &self.backend.features {
            if feature.has_gaps() {
                gaps.push((self.backend.name, feature));
            }
        }
        for feature in &self.integration.features {
            if feature.has_gaps() {
                gaps.push((self.integration.name, feature));
            }
        }
        for feature in &self.frontend.features {
            if feature.has_gaps() {
                gaps.push((self.frontend.name, feature));
            }
        }

        gaps
    }

    /// Print the implementation matrix
    pub fn print_matrix(&self) {
        println!("┌─────────────────────┬───────┬─────────┬────────┬────────┬──────┐");
        println!("│ Feature             │  CLI  │ Desktop │ Mobile │ Daemon │ Node │");
        println!("├─────────────────────┼───────┼─────────┼────────┼────────┼──────┤");

        self.print_layer(&self.backend);
        self.print_layer(&self.integration);
        self.print_layer(&self.frontend);

        println!("└─────────────────────┴───────┴─────────┴────────┴────────┴──────┘");
        println!();
        println!("Legend: ✓ implemented  ◐ partial  ✗ needs work  → via proxy  — n/a");
    }

    fn print_layer(&self, layer: &LayerMatrix) {
        println!("│ {:19} │       │         │        │        │      │", layer.name.to_uppercase());

        for feature in &layer.features {
            let cli = feature.get_status(Platform::Cli);
            let desktop = feature.get_status(Platform::Desktop);
            let mobile = feature.get_status(Platform::Mobile);
            let daemon = feature.get_status(Platform::Daemon);
            let node = feature.get_status(Platform::Node);

            println!(
                "│   {:17} │   {}   │    {}    │   {}    │   {}    │  {}   │",
                feature.name,
                cli.symbol(),
                desktop.symbol(),
                mobile.symbol(),
                daemon.symbol(),
                node.symbol()
            );
        }
    }

    /// Print gaps report
    pub fn print_gaps_report(&self) {
        let gaps = self.all_gaps();

        if gaps.is_empty() {
            println!("No implementation gaps found!");
            return;
        }

        println!("Implementation Gaps Report");
        println!("==========================\n");

        for (layer, feature) in gaps {
            let platforms: Vec<_> = feature
                .platforms_needing_work()
                .iter()
                .map(|p| p.name())
                .collect();

            println!("[{}] {}", layer, feature.name);
            println!("  Needs work on: {}", platforms.join(", "));
            if !feature.depends_on.is_empty() {
                println!("  Depends on: {}", feature.depends_on.join(", "));
            }
            println!();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_all() {
        assert_eq!(Platform::all().len(), 5);
    }

    #[test]
    fn test_status_needs_work() {
        assert!(Status::NotImplemented.needs_work());
        assert!(Status::Partial.needs_work());
        assert!(!Status::Implemented.needs_work());
        assert!(!Status::NotApplicable.needs_work());
    }

    #[test]
    fn test_feature_gaps() {
        let feature = Feature::new("test", "Test", "Test feature")
            .with_status(Platform::Cli, Status::Implemented)
            .with_status(Platform::Mobile, Status::NotImplemented);

        assert!(feature.has_gaps());
        assert_eq!(feature.platforms_needing_work(), vec![Platform::Mobile]);
    }

    #[test]
    fn test_implementation_matrix() {
        let matrix = ImplementationMatrix::current();

        // Should have gaps (Mobile, Desktop UI, etc.)
        assert!(!matrix.all_gaps().is_empty());
    }

    #[test]
    fn test_feature_dependencies() {
        let feature = Feature::new("sdk", "SDK", "Client SDK")
            .depends("network")
            .depends("crypto");

        assert_eq!(feature.depends_on.len(), 2);
        assert!(feature.depends_on.contains(&"network"));
    }
}

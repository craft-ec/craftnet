//! Architecture layers
//!
//! TunnelCraft follows a layered architecture where each layer
//! depends on the layers below it.

/// Architecture layer
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Backend layer - core functionality
    ///
    /// Components:
    /// - Network (libp2p, P2P communication)
    /// - Relay (shard forwarding)
    /// - Exit (HTTP fetching)
    /// - Settlement (Solana integration)
    /// - Crypto (encryption, signatures)
    /// - Erasure (Reed-Solomon coding)
    Backend = 0,

    /// Integration layer - bridges and services
    ///
    /// Components:
    /// - Daemon (background service)
    /// - IPC (inter-process communication)
    /// - FFI/JNI (mobile bindings)
    /// - SDK (client interface)
    Integration = 1,

    /// Frontend layer - user interfaces
    ///
    /// Components:
    /// - CLI (command-line interface)
    /// - Desktop (Electron app)
    /// - Mobile (React Native app)
    Frontend = 2,
}

impl Layer {
    /// Check if this layer includes the given layer
    ///
    /// Higher layers include all lower layers.
    pub fn includes(&self, other: Layer) -> bool {
        *self >= other
    }

    /// Get all layers included by this layer
    pub fn included_layers(&self) -> Vec<Layer> {
        match self {
            Layer::Backend => vec![Layer::Backend],
            Layer::Integration => vec![Layer::Backend, Layer::Integration],
            Layer::Frontend => vec![Layer::Backend, Layer::Integration, Layer::Frontend],
        }
    }

    /// Get the display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Layer::Backend => "Backend",
            Layer::Integration => "Integration",
            Layer::Frontend => "Frontend",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_ordering() {
        assert!(Layer::Frontend > Layer::Integration);
        assert!(Layer::Integration > Layer::Backend);
        assert!(Layer::Frontend > Layer::Backend);
    }

    #[test]
    fn test_layer_includes() {
        assert!(Layer::Frontend.includes(Layer::Backend));
        assert!(Layer::Frontend.includes(Layer::Integration));
        assert!(Layer::Frontend.includes(Layer::Frontend));

        assert!(Layer::Integration.includes(Layer::Backend));
        assert!(Layer::Integration.includes(Layer::Integration));
        assert!(!Layer::Integration.includes(Layer::Frontend));

        assert!(Layer::Backend.includes(Layer::Backend));
        assert!(!Layer::Backend.includes(Layer::Integration));
        assert!(!Layer::Backend.includes(Layer::Frontend));
    }

    #[test]
    fn test_included_layers() {
        assert_eq!(Layer::Backend.included_layers(), vec![Layer::Backend]);
        assert_eq!(
            Layer::Integration.included_layers(),
            vec![Layer::Backend, Layer::Integration]
        );
        assert_eq!(
            Layer::Frontend.included_layers(),
            vec![Layer::Backend, Layer::Integration, Layer::Frontend]
        );
    }
}

//! TunnelCraft CLI
//!
//! Command-line interface for the TunnelCraft VPN client and node operator.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use libp2p::{Multiaddr, PeerId};
use tracing::info;

use tunnelcraft_app::{AppBuilder, AppType, ImplementationMatrix};
use tunnelcraft_client::{SDKConfig, TunnelCraftSDK};
use tunnelcraft_core::HopMode;
use tunnelcraft_daemon::{NodeConfig, NodeService, NodeType};
use tunnelcraft_ipc_client::{IpcClient, DEFAULT_SOCKET_PATH};
use tunnelcraft_keystore::{expand_path, load_or_generate_libp2p_keypair};

/// TunnelCraft - Decentralized Trustless VPN
#[derive(Parser)]
#[command(name = "tunnelcraft")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Socket path for daemon communication
    #[arg(long, default_value = DEFAULT_SOCKET_PATH)]
    socket: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to the TunnelCraft network
    Connect {
        /// Number of relay hops (0-3)
        #[arg(short = 'n', long, default_value = "2")]
        hops: u8,
    },

    /// Disconnect from the network
    Disconnect,

    /// Show connection status
    Status,

    /// Show or manage credits
    Credits {
        #[command(subcommand)]
        action: Option<CreditsAction>,
    },

    /// Make an HTTP request through the tunnel (for testing)
    Request {
        /// HTTP method
        #[arg(short, long, default_value = "GET")]
        method: String,

        /// URL to request
        url: String,

        /// Request body (for POST/PUT)
        #[arg(short, long)]
        body: Option<String>,

        /// Request headers (key:value format)
        #[arg(short = 'H', long)]
        header: Vec<String>,
    },

    /// Start the daemon (usually run by system service)
    Daemon,

    /// Run in standalone mode (SDK direct, no daemon)
    Run {
        /// Number of relay hops (0-3)
        #[arg(short = 'n', long, default_value = "2")]
        hops: u8,

        /// Bootstrap peer address
        #[arg(short, long)]
        bootstrap: Option<String>,

        /// Listen address for libp2p
        #[arg(short, long, default_value = "/ip4/0.0.0.0/tcp/0")]
        listen: String,
    },

    /// Fetch a URL using SDK directly (standalone mode)
    Fetch {
        /// URL to fetch
        url: String,

        /// Number of relay hops (0-3)
        #[arg(short = 'n', long, default_value = "2")]
        hops: u8,

        /// Bootstrap peer address
        #[arg(short, long)]
        bootstrap: Option<String>,
    },

    /// Run as a network node (relay/exit) to earn credits
    Node {
        #[command(subcommand)]
        mode: NodeMode,
    },

    /// Developer tools and diagnostics
    Dev {
        #[command(subcommand)]
        action: DevAction,
    },
}

#[derive(Subcommand)]
enum NodeMode {
    /// Run as relay node only
    Relay {
        /// Listen address
        #[arg(short, long, default_value = "/ip4/0.0.0.0/tcp/9000")]
        listen: String,

        /// Bootstrap peer (format: <peer_id>@<multiaddr>)
        #[arg(short, long)]
        bootstrap: Vec<String>,

        /// Path to keypair file
        #[arg(long, default_value = "~/.tunnelcraft/node.key")]
        keyfile: PathBuf,

        /// Allow being last hop (required for settlement)
        #[arg(long)]
        allow_last_hop: bool,
    },

    /// Run as exit node (also relays)
    Exit {
        /// Listen address
        #[arg(short, long, default_value = "/ip4/0.0.0.0/tcp/9000")]
        listen: String,

        /// Bootstrap peer (format: <peer_id>@<multiaddr>)
        #[arg(short, long)]
        bootstrap: Vec<String>,

        /// Path to keypair file
        #[arg(long, default_value = "~/.tunnelcraft/node.key")]
        keyfile: PathBuf,

        /// HTTP request timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,
    },

    /// Run as full node (relay + exit)
    Full {
        /// Listen address
        #[arg(short, long, default_value = "/ip4/0.0.0.0/tcp/9000")]
        listen: String,

        /// Bootstrap peer (format: <peer_id>@<multiaddr>)
        #[arg(short, long)]
        bootstrap: Vec<String>,

        /// Path to keypair file
        #[arg(long, default_value = "~/.tunnelcraft/node.key")]
        keyfile: PathBuf,

        /// HTTP request timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,
    },

    /// Show node information
    Info {
        /// Path to keypair file
        #[arg(long, default_value = "~/.tunnelcraft/node.key")]
        keyfile: PathBuf,
    },
}

#[derive(Subcommand)]
enum CreditsAction {
    /// Show current credit balance
    Show,
    /// Purchase credits
    Buy {
        /// Amount of credits to purchase
        amount: u64,
    },
}

#[derive(Subcommand)]
enum DevAction {
    /// Show feature implementation matrix
    Matrix,
    /// Show implementation gaps report
    Gaps,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize app with standard startup sequence
    let app_type = match &cli.command {
        Commands::Daemon => AppType::Daemon,
        Commands::Node { .. } => AppType::Node,
        _ => AppType::Cli,
    };

    let _app = AppBuilder::new()
        .name("tunnelcraft")
        .version(env!("CARGO_PKG_VERSION"))
        .app_type(app_type)
        .verbose(cli.verbose)
        .build()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    match cli.command {
        Commands::Connect { hops } => {
            connect(&cli.socket, hops).await?;
        }
        Commands::Disconnect => {
            disconnect(&cli.socket).await?;
        }
        Commands::Status => {
            status(&cli.socket).await?;
        }
        Commands::Credits { action } => {
            credits(&cli.socket, action).await?;
        }
        Commands::Request {
            method,
            url,
            body,
            header,
        } => {
            request(&cli.socket, &method, &url, body, header).await?;
        }
        Commands::Daemon => {
            run_daemon().await?;
        }
        Commands::Run {
            hops,
            bootstrap,
            listen,
        } => {
            run_standalone(hops, bootstrap, listen).await?;
        }
        Commands::Fetch {
            url,
            hops,
            bootstrap,
        } => {
            fetch_standalone(&url, hops, bootstrap).await?;
        }
        Commands::Node { mode } => {
            run_node(mode).await?;
        }
        Commands::Dev { action } => {
            run_dev(action);
        }
    }

    Ok(())
}

// ============================================================================
// Developer Tools
// ============================================================================

fn run_dev(action: DevAction) {
    match action {
        DevAction::Matrix => {
            let matrix = ImplementationMatrix::current();
            matrix.print_matrix();
        }
        DevAction::Gaps => {
            let matrix = ImplementationMatrix::current();
            matrix.print_gaps_report();
        }
    }
}

// ============================================================================
// IPC Commands (using shared ipc-client crate)
// ============================================================================

async fn connect(socket: &PathBuf, hops: u8) -> Result<()> {
    info!("Connecting to TunnelCraft network with {} hops...", hops);

    let client = IpcClient::new(socket.clone());
    let result = client.connect_vpn(hops).await?;

    if result.connected {
        println!("Connected to TunnelCraft network");
        if let Some(exit) = result.exit_node {
            println!("Exit node: {}", exit);
        }
    } else {
        println!("Connection initiated...");
    }

    Ok(())
}

async fn disconnect(socket: &PathBuf) -> Result<()> {
    info!("Disconnecting from TunnelCraft network...");

    let client = IpcClient::new(socket.clone());
    client.disconnect().await?;

    println!("Disconnected from TunnelCraft network");
    Ok(())
}

async fn status(socket: &PathBuf) -> Result<()> {
    let client = IpcClient::new(socket.clone());
    let result = client.status().await?;

    println!("TunnelCraft Status");
    println!("==================");
    println!("State: {}", result.state);
    println!("Connected: {}", result.connected);
    if let Some(exit) = result.exit_node {
        println!("Exit node: {}", exit);
    }
    if let Some(hops) = result.hops {
        println!("Hops: {}", hops);
    }
    if let Some(credits) = result.credits {
        println!("Credits: {}", credits);
    }

    Ok(())
}

async fn credits(socket: &PathBuf, action: Option<CreditsAction>) -> Result<()> {
    let client = IpcClient::new(socket.clone());

    match action {
        Some(CreditsAction::Buy { amount }) => {
            info!("Purchasing {} credits...", amount);
            let result = client.purchase_credits(amount).await?;
            println!("Purchase result: {}", result);
        }
        Some(CreditsAction::Show) | None => {
            let result = client.get_credits().await?;
            println!("Current credits: {}", result.credits);
        }
    }

    Ok(())
}

async fn request(
    socket: &PathBuf,
    method: &str,
    url: &str,
    body: Option<String>,
    headers: Vec<String>,
) -> Result<()> {
    info!("Making {} request to {}", method, url);

    let client = IpcClient::new(socket.clone());

    // Build headers map
    let headers_map: std::collections::HashMap<String, String> = headers
        .iter()
        .filter_map(|h| {
            let parts: Vec<&str> = h.splitn(2, ':').collect();
            if parts.len() == 2 {
                Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
            } else {
                None
            }
        })
        .collect();

    let params = serde_json::json!({
        "method": method,
        "url": url,
        "body": body,
        "headers": headers_map,
    });

    let result = client.send_request("request", Some(params)).await?;

    if let Some(status) = result.get("status") {
        println!("Status: {}", status);
    }
    if let Some(body) = result.get("body") {
        println!("\n{}", body);
    }

    Ok(())
}

// ============================================================================
// Daemon
// ============================================================================

async fn run_daemon() -> Result<()> {
    use tunnelcraft_daemon::{DaemonService, IpcConfig, IpcServer};

    info!("Starting TunnelCraft daemon...");

    let config = IpcConfig::default();
    let service = DaemonService::new().map_err(|e| anyhow::anyhow!("{}", e))?;

    info!("IPC server listening on {:?}", config.socket_path);

    let mut server = IpcServer::new(config);
    server
        .start(service)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Wait for shutdown
    tokio::signal::ctrl_c().await?;
    server.stop().await;

    Ok(())
}

// ============================================================================
// Standalone Mode (direct SDK usage)
// ============================================================================

async fn run_standalone(hops: u8, bootstrap: Option<String>, listen: String) -> Result<()> {
    info!("Running in standalone mode with {} hops", hops);

    let hop_mode = match hops {
        0 => HopMode::Direct,
        1 => HopMode::Light,
        2 => HopMode::Standard,
        _ => HopMode::Paranoid,
    };

    let listen_addr: Multiaddr = listen.parse().context("Invalid listen address")?;

    let mut config = SDKConfig {
        hop_mode,
        listen_addr,
        ..Default::default()
    };

    // Parse bootstrap peer if provided
    if let Some(peer_str) = bootstrap {
        if let Some((peer_id_str, addr_str)) = peer_str.split_once('@') {
            let peer_id: PeerId = peer_id_str.parse().context("Invalid peer ID")?;
            let addr: Multiaddr = addr_str.parse().context("Invalid address")?;
            config.bootstrap_peers.push((peer_id, addr));
        }
    }

    let mut sdk = TunnelCraftSDK::new(config).await?;
    sdk.connect().await?;

    info!("SDK connected. Press Ctrl+C to stop.");

    // Wait for shutdown
    tokio::signal::ctrl_c().await?;
    sdk.disconnect().await;

    Ok(())
}

async fn fetch_standalone(url: &str, hops: u8, bootstrap: Option<String>) -> Result<()> {
    info!("Fetching {} with {} hops", url, hops);

    let hop_mode = match hops {
        0 => HopMode::Direct,
        1 => HopMode::Light,
        2 => HopMode::Standard,
        _ => HopMode::Paranoid,
    };

    let mut config = SDKConfig {
        hop_mode,
        ..Default::default()
    };

    // Parse bootstrap peer if provided
    if let Some(peer_str) = bootstrap {
        if let Some((peer_id_str, addr_str)) = peer_str.split_once('@') {
            let peer_id: PeerId = peer_id_str.parse().context("Invalid peer ID")?;
            let addr: Multiaddr = addr_str.parse().context("Invalid address")?;
            config.bootstrap_peers.push((peer_id, addr));
        }
    }

    let mut sdk = TunnelCraftSDK::new(config).await?;
    sdk.connect().await?;

    info!("SDK connected, making request...");

    match sdk.get(url).await {
        Ok(response) => {
            println!("Status: {}", response.status);
            println!("\n{}", String::from_utf8_lossy(&response.body));
        }
        Err(e) => {
            eprintln!("Request failed: {}", e);
        }
    }

    sdk.disconnect().await;
    Ok(())
}

// ============================================================================
// Node Operations (using shared NodeService from daemon crate)
// ============================================================================

async fn run_node(mode: NodeMode) -> Result<()> {
    match mode {
        NodeMode::Relay {
            listen,
            bootstrap,
            keyfile,
            allow_last_hop,
        } => {
            run_node_with_config(NodeType::Relay, &listen, &bootstrap, &keyfile, allow_last_hop, 30)
                .await
        }
        NodeMode::Exit {
            listen,
            bootstrap,
            keyfile,
            timeout,
        } => run_node_with_config(NodeType::Exit, &listen, &bootstrap, &keyfile, true, timeout).await,
        NodeMode::Full {
            listen,
            bootstrap,
            keyfile,
            timeout,
        } => run_node_with_config(NodeType::Full, &listen, &bootstrap, &keyfile, true, timeout).await,
        NodeMode::Info { keyfile } => show_node_info(&keyfile),
    }
}

fn show_node_info(keyfile: &PathBuf) -> Result<()> {
    let keypair = load_or_generate_libp2p_keypair(keyfile)
        .map_err(|e| anyhow::anyhow!("Failed to load keypair: {}", e))?;
    let peer_id = PeerId::from(keypair.public());

    println!("TunnelCraft Node Information");
    println!("============================");
    println!("Peer ID: {}", peer_id);
    println!("Keyfile: {:?}", expand_path(keyfile));

    Ok(())
}

async fn run_node_with_config(
    node_type: NodeType,
    listen: &str,
    bootstrap: &[String],
    keyfile: &PathBuf,
    allow_last_hop: bool,
    timeout_secs: u64,
) -> Result<()> {
    info!("Starting TunnelCraft node in {:?} mode", node_type);

    // Load or generate libp2p keypair using shared keystore
    let libp2p_keypair = load_or_generate_libp2p_keypair(keyfile)
        .map_err(|e| anyhow::anyhow!("Failed to load keypair: {}", e))?;
    let peer_id = PeerId::from(libp2p_keypair.public());
    info!("Node Peer ID: {}", peer_id);

    // Parse listen address
    let listen_addr: Multiaddr = listen.parse().context("Invalid listen address")?;

    // Parse bootstrap peers
    let bootstrap_peers = parse_bootstrap_peers(bootstrap)?;

    // Create node config
    let config = NodeConfig {
        node_type,
        listen_addr,
        bootstrap_peers,
        allow_last_hop,
        request_timeout: Duration::from_secs(timeout_secs),
    };

    // Create and start node service
    let mut node_service = NodeService::new(config);
    node_service.start(libp2p_keypair).await?;

    info!(
        "Node running on {}. Press Ctrl+C to stop.",
        listen
    );

    // Wait for shutdown
    tokio::signal::ctrl_c().await?;

    // Print stats
    let stats = node_service.stats().await;
    info!("Shards relayed: {}", stats.shards_relayed);
    info!("Shards exited: {}", stats.shards_exited);

    Ok(())
}

/// Parse bootstrap peer strings in format "peer_id@multiaddr"
fn parse_bootstrap_peers(peers: &[String]) -> Result<Vec<(PeerId, Multiaddr)>> {
    let mut result = Vec::new();
    for peer_str in peers {
        if let Some((peer_id_str, addr_str)) = peer_str.split_once('@') {
            let peer_id: PeerId = peer_id_str
                .parse()
                .context("Invalid peer ID in bootstrap")?;
            let addr: Multiaddr = addr_str.parse().context("Invalid address in bootstrap")?;
            result.push((peer_id, addr));
        } else {
            tracing::warn!(
                "Invalid bootstrap format: {}. Expected: <peer_id>@<multiaddr>",
                peer_str
            );
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn test_connect_with_hops() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let matches = cmd.try_get_matches_from(vec!["tunnelcraft", "connect", "-n", "3"]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_run_standalone() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let matches = cmd.try_get_matches_from(vec![
            "tunnelcraft",
            "run",
            "-n",
            "2",
            "-b",
            "12D3KooWQNV9B3aYrwqXfzQA9K6c1AzPLQVLyZsyYqNqXcT7Th5E@/ip4/127.0.0.1/tcp/9000",
        ]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_fetch_standalone() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let matches = cmd.try_get_matches_from(vec![
            "tunnelcraft",
            "fetch",
            "https://example.com",
            "-n",
            "1",
        ]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_credits_show() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let matches = cmd.try_get_matches_from(vec!["tunnelcraft", "credits", "show"]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_credits_buy() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let matches = cmd.try_get_matches_from(vec!["tunnelcraft", "credits", "buy", "100"]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_request_with_headers() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let matches = cmd.try_get_matches_from(vec![
            "tunnelcraft",
            "request",
            "-m",
            "POST",
            "https://api.example.com/data",
            "-H",
            "Content-Type: application/json",
            "-b",
            "{\"key\": \"value\"}",
        ]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_parse_bootstrap_peers() {
        let peers = vec![
            "12D3KooWQNV9B3aYrwqXfzQA9K6c1AzPLQVLyZsyYqNqXcT7Th5E@/ip4/127.0.0.1/tcp/9000"
                .to_string(),
        ];
        let result = parse_bootstrap_peers(&peers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_parse_bootstrap_peers_invalid() {
        let peers = vec!["invalid_format".to_string()];
        let result = parse_bootstrap_peers(&peers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0); // Invalid format is skipped with warning
    }
}

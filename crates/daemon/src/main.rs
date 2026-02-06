//! TunnelCraft Daemon Binary
//!
//! Runs the IPC server for desktop/mobile frontends.

use tunnelcraft_daemon::{DaemonService, IpcServer, IpcConfig, DaemonError};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tunnelcraft=debug"));
    
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
}

#[tokio::main]
async fn main() -> Result<(), DaemonError> {
    init_logging();
    
    tracing::info!("Starting TunnelCraft daemon...");
    
    // Create the daemon service (implements IpcHandler)
    let daemon = DaemonService::new()?;
    
    // Configure IPC server
    let config = IpcConfig::default();
    
    tracing::info!("Daemon starting, will listen on {:?}", config.socket_path);
    
    // Create IPC server with event streaming
    let mut ipc = IpcServer::new(config);
    ipc.set_event_sender(daemon.event_sender());

    // Run until interrupted
    tokio::select! {
        result = ipc.start(daemon) => {
            if let Err(e) = result {
                tracing::error!("IPC server error: {}", e);
                return Err(e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received shutdown signal");
            ipc.stop().await;
        }
    }
    
    tracing::info!("Daemon stopped");
    Ok(())
}

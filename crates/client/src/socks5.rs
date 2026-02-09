//! SOCKS5 proxy server (RFC 1928, CONNECT only, NO AUTH)
//!
//! Listens for incoming browser connections, performs the SOCKS5 handshake,
//! then relays TCP data bidirectionally through the TunnelCraft network.
//!
//! Each SOCKS5 CONNECT creates a long-lived session. Incoming TCP data is
//! buffered into bursts (50ms timeout or 18KB full) and sent as tunnel-mode
//! shards through the VPN.

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use tunnelcraft_core::TunnelMetadata;

use crate::node::TunnelBurst;
use crate::ClientError;

/// Maximum buffer size before flushing a burst (18KB = one full chunk)
const BURST_BUFFER_SIZE: usize = 18 * 1024;

/// Idle timeout before flushing a partial buffer
const BURST_FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

/// SOCKS5 proxy server
pub struct Socks5Server {
    listen_addr: SocketAddr,
    /// Sender to push tunnel bursts to the node's event loop
    burst_tx: mpsc::Sender<TunnelBurst>,
    /// Handle for the listener task
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Socks5Server {
    /// Create a new SOCKS5 server.
    ///
    /// `burst_tx` is the sending side of the channel that feeds into
    /// `TunnelCraftNode`'s event loop via `set_tunnel_burst_rx()`.
    pub fn new(listen_addr: SocketAddr, burst_tx: mpsc::Sender<TunnelBurst>) -> Self {
        Self {
            listen_addr,
            burst_tx,
            handle: None,
        }
    }

    /// Start listening for SOCKS5 connections.
    ///
    /// Returns immediately; the server runs in a background task.
    pub async fn start(&mut self) -> std::io::Result<()> {
        let listener = TcpListener::bind(self.listen_addr).await?;
        let actual_addr = listener.local_addr()?;
        info!("SOCKS5 proxy listening on {}", actual_addr);
        self.listen_addr = actual_addr;

        let burst_tx = self.burst_tx.clone();

        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        debug!("SOCKS5 connection from {}", peer_addr);
                        let tx = burst_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_socks5_connection(stream, tx).await {
                                debug!("SOCKS5 connection from {} ended: {}", peer_addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("SOCKS5 accept error: {}", e);
                    }
                }
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    /// Stop the SOCKS5 server
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            info!("SOCKS5 proxy stopped");
        }
    }

    /// Get the listening address
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }
}

impl Drop for Socks5Server {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Handle a single SOCKS5 connection
async fn handle_socks5_connection(
    mut stream: TcpStream,
    burst_tx: mpsc::Sender<TunnelBurst>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // === SOCKS5 Greeting ===
    // Client sends: VER (1) | NMETHODS (1) | METHODS (1..255)
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;

    if header[0] != 0x05 {
        return Err(format!("Unsupported SOCKS version: {}", header[0]).into());
    }

    let nmethods = header[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await?;

    // We only support NO AUTH (0x00)
    if !methods.contains(&0x00) {
        // Reply: no acceptable methods
        stream.write_all(&[0x05, 0xFF]).await?;
        return Err("Client does not support NO AUTH".into());
    }

    // Reply: NO AUTH selected
    stream.write_all(&[0x05, 0x00]).await?;

    // === SOCKS5 Request ===
    // Client sends: VER (1) | CMD (1) | RSV (1) | ATYP (1) | DST.ADDR | DST.PORT (2)
    let mut request_header = [0u8; 4];
    stream.read_exact(&mut request_header).await?;

    if request_header[0] != 0x05 {
        return Err("Invalid SOCKS5 request version".into());
    }

    if request_header[1] != 0x01 {
        // Only CONNECT (0x01) is supported
        // Reply with command not supported
        stream.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
        return Err(format!("Unsupported SOCKS5 command: {}", request_header[1]).into());
    }

    // Parse destination address
    let host = match request_header[3] {
        0x01 => {
            // IPv4
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await?;
            format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
        }
        0x03 => {
            // Domain name
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await?;
            let len = len_buf[0] as usize;
            let mut domain = vec![0u8; len];
            stream.read_exact(&mut domain).await?;
            String::from_utf8(domain)?
        }
        0x04 => {
            // IPv6
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await?;
            // Format as colon-separated hex pairs
            let parts: Vec<String> = (0..8)
                .map(|i| format!("{:x}", u16::from_be_bytes([addr[i * 2], addr[i * 2 + 1]])))
                .collect();
            parts.join(":")
        }
        _ => {
            stream.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
            return Err(format!("Unsupported address type: {}", request_header[3]).into());
        }
    };

    // Read port (2 bytes, big-endian)
    let mut port_buf = [0u8; 2];
    stream.read_exact(&mut port_buf).await?;
    let port = u16::from_be_bytes(port_buf);

    debug!("SOCKS5 CONNECT to {}:{}", host, port);

    // Reply with success (bound address = 0.0.0.0:0)
    // VER (1) | REP (1) | RSV (1) | ATYP (1) | BND.ADDR (4) | BND.PORT (2)
    stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;

    // === Bidirectional relay ===
    // Generate session_id for this SOCKS5 connection
    let session_id = {
        let mut id = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut id);
        id
    };

    info!(
        "SOCKS5 session {} relaying to {}:{}",
        hex::encode(&session_id[..8]),
        host,
        port
    );

    // Relay loop: read from browser, send through tunnel, write response back
    let result = relay_loop(&mut stream, &host, port, session_id, &burst_tx).await;

    // Send close signal
    let close_metadata = TunnelMetadata {
        host: String::new(),
        port: 0,
        session_id,
        is_close: true,
    };

    let (close_tx, _close_rx) = mpsc::channel(1);
    let _ = burst_tx.send(TunnelBurst {
        metadata: close_metadata,
        data: Vec::new(),
        response_tx: close_tx,
    }).await;

    debug!(
        "SOCKS5 session {} ended",
        hex::encode(&session_id[..8])
    );

    result
}

/// Bidirectional relay loop between browser socket and tunnel
async fn relay_loop(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    session_id: [u8; 32],
    burst_tx: &mpsc::Sender<TunnelBurst>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = vec![0u8; BURST_BUFFER_SIZE];

    loop {
        // Read from browser with a timeout to allow periodic flushing
        let n = match tokio::time::timeout(
            BURST_FLUSH_TIMEOUT,
            stream.read(&mut buf),
        ).await {
            Ok(Ok(0)) => {
                // Browser closed connection
                return Ok(());
            }
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                return Err(e.into());
            }
            Err(_) => {
                // Timeout — no data from browser, continue loop
                continue;
            }
        };

        let data = buf[..n].to_vec();

        // Create a response channel for this burst
        let (response_tx, mut response_rx) = mpsc::channel::<std::result::Result<Vec<u8>, ClientError>>(1);

        let metadata = TunnelMetadata {
            host: host.to_string(),
            port,
            session_id,
            is_close: false,
        };

        // Send burst to node
        if burst_tx.send(TunnelBurst {
            metadata,
            data,
            response_tx,
        }).await.is_err() {
            return Err("Node channel closed".into());
        }

        // Wait for response bytes with timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            response_rx.recv(),
        ).await {
            Ok(Some(Ok(response_bytes))) => {
                if !response_bytes.is_empty() {
                    stream.write_all(&response_bytes).await?;
                }
            }
            Ok(Some(Err(e))) => {
                warn!("Tunnel error for session {}: {}", hex::encode(&session_id[..8]), e);
                return Err(format!("Tunnel error: {}", e).into());
            }
            Ok(None) => {
                // Channel dropped
                return Err("Response channel closed".into());
            }
            Err(_) => {
                warn!("Tunnel response timeout for session {}", hex::encode(&session_id[..8]));
                return Err("Tunnel response timeout".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_socks5_server_creation() {
        let (tx, _rx) = mpsc::channel(10);
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = Socks5Server::new(addr, tx);
        assert_eq!(server.listen_addr().port(), 0);
    }
}

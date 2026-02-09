//! TCP tunnel handler for exit node
//!
//! Manages TCP sessions initiated by SOCKS5 tunnel-mode shards.
//! Each session maps a `session_id` to a live TCP connection to the
//! destination host. Request bytes are piped to the destination and
//! response bytes are read back and returned.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

use tunnelcraft_core::{Id, TunnelMetadata};

use crate::{ExitError, Result};

/// Maximum bytes to read from a TCP destination per burst
const MAX_RESPONSE_BYTES: usize = 256 * 1024; // 256 KB

/// Idle timeout for reading response bytes from destination
const READ_IDLE_TIMEOUT: Duration = Duration::from_millis(100);

/// Active TCP session to a destination
struct TcpSession {
    stream: TcpStream,
    last_activity: Instant,
}

/// TCP tunnel handler managing session pool
pub struct TunnelHandler {
    sessions: HashMap<Id, TcpSession>,
}

impl TunnelHandler {
    /// Create a new tunnel handler
    pub fn new(_keypair: tunnelcraft_crypto::SigningKeypair) -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Process tunnel data: connect, write, read, return raw response bytes.
    ///
    /// The caller (ExitHandler) is responsible for creating response shards.
    pub async fn process_tunnel_bytes(
        &mut self,
        metadata: &TunnelMetadata,
        data: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let session_id = metadata.session_id;

        // Handle close signal
        if metadata.is_close {
            if self.sessions.remove(&session_id).is_some() {
                debug!(
                    "Tunnel session {} closed by client",
                    hex::encode(&session_id[..8])
                );
            }
            return Ok(Vec::new());
        }

        // Get or create session
        #[allow(clippy::map_entry)]
        if !self.sessions.contains_key(&session_id) {
            let addr = format!("{}:{}", metadata.host, metadata.port);
            debug!("Opening tunnel to {} for session {}", addr, hex::encode(&session_id[..8]));

            let stream = tokio::time::timeout(
                Duration::from_secs(10),
                TcpStream::connect(&addr),
            )
            .await
            .map_err(|_| ExitError::Timeout)?
            .map_err(|e| ExitError::TunnelConnectFailed(format!("{}: {}", addr, e)))?;

            self.sessions.insert(session_id, TcpSession {
                stream,
                last_activity: Instant::now(),
            });

            info!("Tunnel session {} established to {}", hex::encode(&session_id[..8]), addr);
        }

        let session = self.sessions.get_mut(&session_id).unwrap();
        session.last_activity = Instant::now();

        // Write request data to destination
        if !data.is_empty() {
            session.stream.write_all(&data).await
                .map_err(|e| ExitError::TunnelIoError(e.to_string()))?;
        }

        // Read response bytes with idle timeout
        let mut response_buf = vec![0u8; MAX_RESPONSE_BYTES];
        let mut total_read = 0usize;

        loop {
            if total_read >= MAX_RESPONSE_BYTES {
                break;
            }

            match tokio::time::timeout(
                READ_IDLE_TIMEOUT,
                session.stream.read(&mut response_buf[total_read..]),
            ).await {
                Ok(Ok(0)) => {
                    debug!("Tunnel destination closed connection for session {}", hex::encode(&session_id[..8]));
                    break;
                }
                Ok(Ok(n)) => {
                    total_read += n;
                }
                Ok(Err(e)) => {
                    warn!("Tunnel read error for session {}: {}", hex::encode(&session_id[..8]), e);
                    break;
                }
                Err(_) => {
                    break;
                }
            }
        }

        response_buf.truncate(total_read);
        Ok(response_buf)
    }

    /// Remove sessions idle longer than `max_age`
    pub fn clear_stale(&mut self, max_age: Duration) {
        let before = self.sessions.len();
        let now = Instant::now();
        self.sessions.retain(|_, session| {
            now.duration_since(session.last_activity) < max_age
        });
        let removed = before - self.sessions.len();
        if removed > 0 {
            warn!("Cleared {} stale tunnel sessions", removed);
        }
    }

    /// Number of active tunnel sessions
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

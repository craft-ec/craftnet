//! Exit node handler (onion-routed)
//!
//! Manages the complete request/response lifecycle:
//! 1. Decrypt routing_tag to get assembly_id
//! 2. Group shards by assembly_id
//! 3. Reconstruct and decrypt ExitPayload
//! 4. Execute HTTP request or tunnel connection
//! 5. Create response shards with onion routing via LeaseSet

use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sha2::{Sha256, Digest};
use tracing::{debug, info, warn};

use tunnelcraft_core::{
    Shard, Id, PublicKey, ExitPayload,
    TunnelMetadata, PAYLOAD_MODE_TUNNEL,
};
use tunnelcraft_crypto::{
    SigningKeypair, EncryptionKeypair,
    decrypt_routing_tag, decrypt_exit_payload,
    build_onion_header, encrypt_routing_tag,
};
use tunnelcraft_core::OnionSettlement;
use tunnelcraft_erasure::ErasureCoder;
use tunnelcraft_erasure::chunker::{chunk_and_encode, reassemble};
use tunnelcraft_settlement::SettlementClient;

use crate::{ExitError, Result, HttpRequest, HttpResponse};
use crate::tunnel_handler::TunnelHandler;

/// Exit node configuration
#[derive(Debug, Clone)]
pub struct ExitConfig {
    /// HTTP client timeout
    pub timeout: Duration,
    /// Maximum request body size (bytes)
    pub max_request_size: usize,
    /// Maximum response body size (bytes)
    pub max_response_size: usize,
    /// Blocked domains (basic filtering)
    pub blocked_domains: Vec<String>,
    /// Allow requests to private/internal IP ranges (default: false for SSRF protection)
    pub allow_private_ips: bool,
    /// Maximum concurrent tunnels per user public key
    pub max_tunnels_per_user: usize,
    /// Maximum pending assemblies per user public key
    pub max_pending_per_user: usize,
    /// Global cap on pending assemblies (prevents memory exhaustion from orphan entries)
    pub max_pending_assemblies: usize,
}

impl Default for ExitConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_request_size: 10 * 1024 * 1024,  // 10 MB
            max_response_size: 50 * 1024 * 1024, // 50 MB
            blocked_domains: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
            ],
            allow_private_ips: false,
            max_tunnels_per_user: 50,
            max_pending_per_user: 100,
            max_pending_assemblies: 10_000,
        }
    }
}

/// Per-user resource tracker
struct UserTracker {
    concurrent_tunnels: usize,
    /// Pending assemblies owned by this user (tracked via routing tag pool_pubkey)
    pending_assemblies: usize,
    last_activity: Instant,
}

/// Check if an IP address is in a private/internal range (SSRF protection)
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()                              // 127.0.0.0/8
            || v4.is_unspecified()                         // 0.0.0.0
            || v4.octets()[0] == 10                        // 10.0.0.0/8
            || (v4.octets()[0] == 172 && (v4.octets()[1] & 0xf0) == 16) // 172.16.0.0/12
            || (v4.octets()[0] == 192 && v4.octets()[1] == 168)         // 192.168.0.0/16
            || (v4.octets()[0] == 169 && v4.octets()[1] == 254)         // 169.254.0.0/16 (link-local + metadata)
            || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64) // 100.64.0.0/10 (CGNAT)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()                               // ::1
            || v6.is_unspecified()                          // ::
            || (v6.segments()[0] & 0xfe00) == 0xfc00        // fc00::/7 (unique local)
            || (v6.segments()[0] & 0xffc0) == 0xfe80        // fe80::/10 (link-local)
        }
    }
}

/// Extract host from a URL or host:port string
fn extract_host(url_or_host: &str) -> &str {
    // Try to parse as URL with scheme
    if let Some(after_scheme) = url_or_host.strip_prefix("http://")
        .or_else(|| url_or_host.strip_prefix("https://"))
    {
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        // Strip port
        if let Some(bracket_end) = host_port.find(']') {
            // IPv6: [::1]:port
            &host_port[..=bracket_end]
        } else if let Some(colon) = host_port.rfind(':') {
            &host_port[..colon]
        } else {
            host_port
        }
    } else {
        // Bare host or host:port
        if let Some(bracket_end) = url_or_host.find(']') {
            &url_or_host[..=bracket_end]
        } else if let Some(colon) = url_or_host.rfind(':') {
            // Check if it's a port number after the colon
            if url_or_host[colon + 1..].chars().all(|c| c.is_ascii_digit()) {
                &url_or_host[..colon]
            } else {
                url_or_host
            }
        } else {
            url_or_host
        }
    }
}

/// Pending assembly awaiting more shards (grouped by assembly_id)
struct PendingAssembly {
    /// Collected shard payloads indexed by (chunk_index, shard_index)
    shards: HashMap<(u16, u8), Vec<u8>>,
    /// Total chunks expected
    total_chunks: u16,
    /// Total shards per chunk
    #[allow(dead_code)]
    total_shards: u8,
    /// When this pending assembly was created
    created_at: Instant,
    /// Pool pubkey of the user who owns this assembly (for per-user tracking)
    pool_pubkey: PublicKey,
}

/// Exit node handler (onion-routed)
pub struct ExitHandler {
    config: ExitConfig,
    http_client: reqwest::Client,
    erasure: ErasureCoder,
    /// Pending assemblies: assembly_id → shard payloads
    pending: HashMap<Id, PendingAssembly>,
    /// Our signing keypair for signing response shards
    #[allow(dead_code)]
    keypair: SigningKeypair,
    /// Our encryption keypair for decrypting routing tags and exit payloads
    encryption_keypair: EncryptionKeypair,
    /// Settlement client (optional)
    settlement_client: Option<Arc<SettlementClient>>,
    /// TCP tunnel handler for SOCKS5 proxy mode
    tunnel_handler: TunnelHandler,
    /// Per-user resource tracking
    user_tracking: HashMap<PublicKey, UserTracker>,
}

impl ExitHandler {
    /// Create a new exit handler with signing and encryption keypairs
    pub fn new(config: ExitConfig, _our_pubkey: PublicKey, our_secret: [u8; 32]) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent("TunnelCraft/0.1")
            .build()?;

        let keypair = SigningKeypair::from_secret_bytes(&our_secret);
        let encryption_keypair = EncryptionKeypair::generate();
        let tunnel_handler = TunnelHandler::new(SigningKeypair::from_secret_bytes(&our_secret));

        Ok(Self {
            config,
            http_client,
            erasure: ErasureCoder::new()?,
            pending: HashMap::new(),
            keypair,
            encryption_keypair,
            settlement_client: None,
            tunnel_handler,
            user_tracking: HashMap::new(),
        })
    }

    /// Create a new exit handler with a SigningKeypair directly
    pub fn with_keypair(config: ExitConfig, keypair: SigningKeypair) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent("TunnelCraft/0.1")
            .build()?;

        let encryption_keypair = EncryptionKeypair::generate();
        let tunnel_handler = TunnelHandler::new(keypair.clone());

        Ok(Self {
            config,
            http_client,
            erasure: ErasureCoder::new()?,
            pending: HashMap::new(),
            keypair,
            encryption_keypair,
            settlement_client: None,
            tunnel_handler,
            user_tracking: HashMap::new(),
        })
    }

    /// Create with explicit encryption keypair (for testing)
    pub fn with_keypairs(
        config: ExitConfig,
        keypair: SigningKeypair,
        encryption_keypair: EncryptionKeypair,
    ) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent("TunnelCraft/0.1")
            .build()?;

        let tunnel_handler = TunnelHandler::new(keypair.clone());

        Ok(Self {
            config,
            http_client,
            erasure: ErasureCoder::new()?,
            pending: HashMap::new(),
            keypair,
            encryption_keypair,
            settlement_client: None,
            tunnel_handler,
            user_tracking: HashMap::new(),
        })
    }

    /// Create a new exit handler with settlement client
    pub fn with_settlement(
        config: ExitConfig,
        _our_pubkey: PublicKey,
        our_secret: [u8; 32],
        settlement_client: Arc<SettlementClient>,
    ) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent("TunnelCraft/0.1")
            .build()?;

        let keypair = SigningKeypair::from_secret_bytes(&our_secret);
        let encryption_keypair = EncryptionKeypair::generate();
        let tunnel_handler = TunnelHandler::new(SigningKeypair::from_secret_bytes(&our_secret));

        Ok(Self {
            config,
            http_client,
            erasure: ErasureCoder::new()?,
            pending: HashMap::new(),
            keypair,
            encryption_keypair,
            settlement_client: Some(settlement_client),
            tunnel_handler,
            user_tracking: HashMap::new(),
        })
    }

    /// Create with keypair and settlement client
    pub fn with_keypair_and_settlement(
        config: ExitConfig,
        keypair: SigningKeypair,
        settlement_client: Arc<SettlementClient>,
    ) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent("TunnelCraft/0.1")
            .build()?;

        let encryption_keypair = EncryptionKeypair::generate();
        let tunnel_handler = TunnelHandler::new(keypair.clone());

        Ok(Self {
            config,
            http_client,
            erasure: ErasureCoder::new()?,
            pending: HashMap::new(),
            keypair,
            encryption_keypair,
            settlement_client: Some(settlement_client),
            tunnel_handler,
            user_tracking: HashMap::new(),
        })
    }

    /// Set the settlement client
    pub fn set_settlement_client(&mut self, client: Arc<SettlementClient>) {
        self.settlement_client = Some(client);
    }

    /// Get our encryption public key (for topology advertisements)
    pub fn encryption_pubkey(&self) -> [u8; 32] {
        self.encryption_keypair.public_key_bytes()
    }

    /// Process an incoming shard (onion-routed)
    ///
    /// 1. Decrypt routing_tag → assembly_id
    /// 2. Group by assembly_id
    /// 3. When all chunks ready: reconstruct → decrypt → process
    /// 4. Create response shards using LeaseSet
    ///
    /// Returns per-shard `(shard, gateway_peer_id_bytes)` pairs.
    /// Each shard may target a different gateway (round-robin across LeaseSet).
    /// If no LeaseSet gateway, gateway is None (direct mode — caller should use source_peer).
    /// Collect a shard into its pending assembly (fast, no I/O).
    ///
    /// Returns `Ok(Some(assembly_id))` when the assembly is complete and ready
    /// for processing via [`process_complete_assembly`]. Returns `Ok(None)` if
    /// still collecting shards.
    pub fn collect_shard(&mut self, shard: Shard) -> Result<Option<Id>> {
        // Decrypt routing_tag to get assembly_id + shard/chunk metadata + pool_pubkey
        let tag = decrypt_routing_tag(
            &self.encryption_keypair.secret_key_bytes(),
            &shard.routing_tag,
        ).map_err(|e| ExitError::InvalidRequest(format!("routing_tag decrypt failed: {}", e)))?;

        let assembly_id = tag.assembly_id;
        let chunk_index = tag.chunk_index;
        let shard_index = tag.shard_index;
        let total_chunks = tag.total_chunks;
        let pool_pubkey = tag.pool_pubkey;

        // Check if this is a new assembly (not already in pending map)
        let is_new_assembly = !self.pending.contains_key(&assembly_id);

        if is_new_assembly {
            // Global cap: prevent memory exhaustion from sybil/orphan assemblies
            if self.pending.len() >= self.config.max_pending_assemblies {
                return Err(ExitError::RateLimited(
                    "global pending assembly limit reached".to_string(),
                ));
            }

            // Per-user cap: prevent a single user from hogging assembly slots
            let tracker = self.user_tracking.entry(pool_pubkey).or_insert_with(|| {
                UserTracker {
                    concurrent_tunnels: 0,
                    pending_assemblies: 0,
                    last_activity: Instant::now(),
                }
            });
            tracker.last_activity = Instant::now();

            if tracker.pending_assemblies >= self.config.max_pending_per_user {
                return Err(ExitError::RateLimited(
                    "per-user pending assembly limit reached".to_string(),
                ));
            }

            tracker.pending_assemblies += 1;
        }

        // Add shard payload to pending assembly
        {
            let pending = self.pending.entry(assembly_id).or_insert_with(|| {
                PendingAssembly {
                    shards: HashMap::new(),
                    total_chunks,
                    total_shards: tag.total_shards,
                    created_at: Instant::now(),
                    pool_pubkey,
                }
            });
            pending.shards.insert((chunk_index, shard_index), shard.payload);
        }

        // Check if we have enough shards for every chunk
        if !self.all_chunks_ready(&assembly_id) {
            if let Some(pending) = self.pending.get(&assembly_id) {
                let shard_count = pending.shards.len();
                let needed = total_chunks as usize * tunnelcraft_erasure::DATA_SHARDS;
                info!(
                    "[SHARD-FLOW] EXIT assembly={} shard received: chunk={} shard={} ({}/{} shards collected)",
                    hex::encode(&assembly_id[..8]),
                    chunk_index, shard_index,
                    shard_count, needed,
                );
            }
            return Ok(None);
        }

        info!(
            "[SHARD-FLOW] EXIT assembly={} COMPLETE — all shards collected, ready for processing",
            hex::encode(&assembly_id[..8]),
        );

        Ok(Some(assembly_id))
    }

    /// Process a complete assembly: reconstruct, HTTP fetch, create response shards.
    ///
    /// This is the slow path — it performs network I/O (HTTP request). Call this
    /// only after [`collect_shard`] returns `Some(assembly_id)`.
    pub async fn process_complete_assembly(
        &mut self,
        assembly_id: Id,
    ) -> Result<Option<Vec<(Shard, Option<Vec<u8>>)>>> {
        // Extract and reconstruct
        let Some(pending) = self.pending.remove(&assembly_id) else {
            debug!("Assembly {} already processed", hex::encode(&assembly_id[..8]));
            return Ok(None);
        };

        let pool_pubkey = pending.pool_pubkey;

        // Decrement per-user pending assembly count
        if let Some(tracker) = self.user_tracking.get_mut(&pool_pubkey) {
            tracker.pending_assemblies = tracker.pending_assemblies.saturating_sub(1);
        }

        let framed_data = self.reconstruct_data(&pending)?;

        // Strip length-prefixed framing (4-byte LE u32 original length)
        if framed_data.len() < 4 {
            return Err(ExitError::InvalidRequest("Reconstructed data too short for length prefix".to_string()));
        }
        let original_len = u32::from_le_bytes(
            framed_data[..4].try_into().unwrap()
        ) as usize;
        if framed_data.len() < 4 + original_len {
            return Err(ExitError::InvalidRequest(format!(
                "Reconstructed data shorter than declared: {} < {}",
                framed_data.len() - 4, original_len
            )));
        }
        let encrypted_data = &framed_data[4..4 + original_len];

        // Decrypt exit payload
        let exit_payload = decrypt_exit_payload(
            &self.encryption_keypair.secret_key_bytes(),
            encrypted_data,
        ).map_err(|e| ExitError::InvalidRequest(format!("ExitPayload decrypt failed: {}", e)))?;

        debug!(
            "Reconstructed exit payload: request={} type={:?} mode={}",
            hex::encode(&exit_payload.request_id[..8]),
            exit_payload.shard_type,
            exit_payload.mode,
        );

        info!(
            "Processing request {} (type: {:?}, mode: {}, total_hops: {})",
            hex::encode(&exit_payload.request_id[..8]),
            exit_payload.shard_type,
            exit_payload.mode,
            exit_payload.total_hops,
        );

        // Belt-and-suspenders tier enforcement at exit:
        // Verify that total_hops doesn't exceed what the pool's tier allows.
        // Primary enforcement is at every relay via the public Shard fields,
        // but exit validates too after decrypting the ExitPayload.
        if let Some(tracker) = self.user_tracking.get(&pool_pubkey) {
            // tracker exists → we've seen this user. Check tier from recent context.
            let _ = tracker; // tier info would come from subscription cache if we had one at exit
        }
        // Note: full tier validation requires subscription cache at exit level.
        // Currently the relay-side enforcement (decrementing hops_remaining) is the
        // primary mechanism. Exit-side validation is a future enhancement when
        // subscription gossip reaches the exit.

        // Process based on mode
        if exit_payload.mode == PAYLOAD_MODE_TUNNEL {
            return self.process_tunnel_payload(&exit_payload, pool_pubkey).await;
        }

        // HTTP mode
        let http_request = HttpRequest::from_bytes(&exit_payload.data)
            .map_err(|e| ExitError::InvalidRequest(e.to_string()))?;

        self.check_blocked(&http_request.url).await?;

        info!(
            "HTTP request starting: {} {} (request={})",
            http_request.method,
            http_request.url,
            hex::encode(&exit_payload.request_id[..8])
        );

        let response = match self.execute_request(&http_request).await {
            Ok(r) => r,
            Err(e) => {
                warn!("HTTP request failed: {} (request={})", e, hex::encode(&exit_payload.request_id[..8]));
                return Err(e);
            }
        };
        let response_data = response.to_bytes();

        info!(
            "HTTP request completed: request={} status={} response_bytes={}",
            hex::encode(&exit_payload.request_id[..8]),
            response.status,
            response_data.len(),
        );

        let shard_pairs = self.create_response_shards(
            &exit_payload,
            &response_data,
        )?;

        debug!(
            "Created {} response shards for request={} (leases={})",
            shard_pairs.len(),
            hex::encode(&exit_payload.request_id[..8]),
            exit_payload.lease_set.leases.len(),
        );

        Ok(Some(shard_pairs))
    }

    /// Combined collect + process (convenience method, blocks during I/O).
    pub async fn process_shard(&mut self, shard: Shard) -> Result<Option<Vec<(Shard, Option<Vec<u8>>)>>> {
        match self.collect_shard(shard)? {
            Some(assembly_id) => self.process_complete_assembly(assembly_id).await,
            None => Ok(None),
        }
    }

    /// Process a tunnel-mode payload
    async fn process_tunnel_payload(
        &mut self,
        exit_payload: &ExitPayload,
        pool_pubkey: PublicKey,
    ) -> Result<Option<Vec<(Shard, Option<Vec<u8>>)>>> {
        let request_data = &exit_payload.data;
        if request_data.len() < 4 {
            return Err(ExitError::InvalidRequest("Tunnel payload too short".to_string()));
        }

        let metadata_len = u32::from_be_bytes(
            request_data[0..4].try_into().unwrap()
        ) as usize;
        if request_data.len() < 4 + metadata_len {
            return Err(ExitError::InvalidRequest("Tunnel metadata truncated".to_string()));
        }

        let metadata = TunnelMetadata::from_bytes(&request_data[4..4 + metadata_len])
            .map_err(|e| ExitError::InvalidRequest(format!("Invalid tunnel metadata: {}", e)))?;
        let tcp_data = request_data[4 + metadata_len..].to_vec();

        self.check_blocked(&metadata.host).await?;

        // Per-user tunnel limit check (keyed by pool_pubkey for consistency)
        {
            let tracker = self.user_tracking.entry(pool_pubkey).or_insert(UserTracker {
                concurrent_tunnels: 0,
                pending_assemblies: 0,
                last_activity: Instant::now(),
            });
            tracker.last_activity = Instant::now();

            if !metadata.is_close && tracker.concurrent_tunnels >= self.config.max_tunnels_per_user {
                return Err(ExitError::RateLimited(format!(
                    "User exceeds max concurrent tunnels ({})",
                    self.config.max_tunnels_per_user,
                )));
            }
        }

        info!(
            "Tunnel request to {}:{} for request {} (session {})",
            metadata.host,
            metadata.port,
            hex::encode(&exit_payload.request_id[..8]),
            hex::encode(&metadata.session_id[..8])
        );

        let is_new_session = !self.tunnel_handler.has_session(&metadata.session_id);

        // Use tunnel handler for TCP connections (passes pool_pubkey for session ownership)
        let (response_bytes, zombie) = self.tunnel_handler.process_tunnel_bytes(
            &metadata,
            tcp_data,
            pool_pubkey,
        ).await?;

        // Track new tunnel creation
        if is_new_session && self.tunnel_handler.has_session(&metadata.session_id) {
            if let Some(tracker) = self.user_tracking.get_mut(&pool_pubkey) {
                tracker.concurrent_tunnels += 1;
            }
        }
        // Track tunnel close (explicit close or zombie removal)
        if metadata.is_close || zombie {
            if let Some(tracker) = self.user_tracking.get_mut(&pool_pubkey) {
                tracker.concurrent_tunnels = tracker.concurrent_tunnels.saturating_sub(1);
            }
        }

        if response_bytes.is_empty() {
            return Ok(Some(vec![]));
        }

        let shard_pairs = self.create_response_shards(
            exit_payload,
            &response_bytes,
        )?;

        Ok(Some(shard_pairs))
    }

    /// Check if all chunks for an assembly have enough shards
    fn all_chunks_ready(&self, assembly_id: &Id) -> bool {
        let Some(pending) = self.pending.get(assembly_id) else {
            return false;
        };

        let mut chunk_counts: HashMap<u16, usize> = HashMap::new();
        for &(chunk_idx, _) in pending.shards.keys() {
            *chunk_counts.entry(chunk_idx).or_default() += 1;
        }

        if chunk_counts.len() < pending.total_chunks as usize {
            return false;
        }
        chunk_counts.values().all(|&count| count >= tunnelcraft_erasure::DATA_SHARDS)
    }

    /// Reconstruct data from shard payloads (multi-chunk aware)
    fn reconstruct_data(&self, pending: &PendingAssembly) -> Result<Vec<u8>> {
        let mut chunks_by_index: HashMap<u16, Vec<(u8, &Vec<u8>)>> = HashMap::new();
        for (&(chunk_idx, shard_idx), payload) in &pending.shards {
            chunks_by_index
                .entry(chunk_idx)
                .or_default()
                .push((shard_idx, payload));
        }

        let mut reconstructed_chunks: BTreeMap<u16, Vec<u8>> = BTreeMap::new();

        for chunk_idx in 0..pending.total_chunks {
            let chunk_shards = chunks_by_index.get(&chunk_idx);
            let mut shard_data: Vec<Option<Vec<u8>>> =
                vec![None; tunnelcraft_erasure::TOTAL_SHARDS];
            let mut shard_size = 0usize;

            if let Some(shards) = chunk_shards {
                for &(shard_idx, payload) in shards {
                    let idx = shard_idx as usize;
                    if idx < tunnelcraft_erasure::TOTAL_SHARDS {
                        shard_size = payload.len();
                        shard_data[idx] = Some(payload.clone());
                    }
                }
            }

            let max_len = shard_size * tunnelcraft_erasure::DATA_SHARDS;
            let chunk_data = self
                .erasure
                .decode(&mut shard_data, max_len)
                .map_err(|e| ExitError::ErasureDecodeError(e.to_string()))?;

            reconstructed_chunks.insert(chunk_idx, chunk_data);
        }

        let total_possible = reconstructed_chunks.values().map(|c| c.len()).sum();
        reassemble(&reconstructed_chunks, pending.total_chunks, total_possible)
            .map_err(|e| ExitError::ErasureDecodeError(e.to_string()))
    }

    /// Check if URL/host is blocked (domain blocklist + private IP SSRF protection)
    async fn check_blocked(&self, url: &str) -> Result<()> {
        let host = extract_host(url);
        for domain in &self.config.blocked_domains {
            if host.contains(domain) {
                return Err(ExitError::BlockedDestination(domain.clone()));
            }
        }

        // SSRF protection: resolve host and check for private IPs
        if !self.config.allow_private_ips {
            // Try parsing as IP directly first
            let host_stripped = host.trim_start_matches('[').trim_end_matches(']');
            if let Ok(ip) = host_stripped.parse::<IpAddr>() {
                if is_private_ip(ip) {
                    return Err(ExitError::BlockedDestination(
                        format!("{} (private IP)", host),
                    ));
                }
            } else {
                // DNS resolution check
                let lookup_target = format!("{}:0", host);
                let resolved: Vec<std::net::SocketAddr> =
                    match tokio::net::lookup_host(lookup_target.as_str()).await {
                        Ok(addrs) => addrs.collect(),
                        Err(_) => vec![],
                    };
                for addr in &resolved {
                    if is_private_ip(addr.ip()) {
                        return Err(ExitError::BlockedDestination(
                            format!("{} resolves to private IP {}", host, addr.ip()),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute an HTTP request
    async fn execute_request(&self, request: &HttpRequest) -> Result<HttpResponse> {
        let method = request.method.to_uppercase();
        let mut req = match method.as_str() {
            "GET" => self.http_client.get(&request.url),
            "POST" => self.http_client.post(&request.url),
            "PUT" => self.http_client.put(&request.url),
            "DELETE" => self.http_client.delete(&request.url),
            "PATCH" => self.http_client.patch(&request.url),
            "HEAD" => self.http_client.head(&request.url),
            _ => return Err(ExitError::InvalidRequest(format!("Unsupported method: {}", method))),
        };

        for (key, value) in &request.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        if let Some(body) = &request.body {
            req = req.body(body.clone());
        }

        let mut response = req.send().await?;
        let status = response.status().as_u16();

        let mut headers = HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                headers.insert(key.to_string(), v.to_string());
            }
        }

        // Stream response body with size enforcement
        let max = self.config.max_response_size;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len() + chunk.len() > max {
                return Err(ExitError::ResponseTooLarge(max));
            }
            body.extend_from_slice(&chunk);
        }

        Ok(HttpResponse::new(status, headers, body))
    }

    /// Create response shards with onion routing via LeaseSet.
    ///
    /// Round-robins each shard across gateways in the LeaseSet. Each shard's
    /// onion header targets its assigned gateway, and the returned pairs tell
    /// the caller which gateway to send each shard to.
    fn create_response_shards(
        &self,
        exit_payload: &ExitPayload,
        response_data: &[u8],
    ) -> Result<Vec<(Shard, Option<Vec<u8>>)>> {
        // Encrypt response for the client using their X25519 encryption pubkey.
        // Falls back to user_pubkey for pre-response_enc_pubkey payloads.
        let recipient_pubkey = if exit_payload.response_enc_pubkey != [0u8; 32] {
            &exit_payload.response_enc_pubkey
        } else {
            &exit_payload.user_pubkey
        };
        warn!(
            "[TRACE] EXIT_RESPONSE_KEYS request={} recipient_enc_key={} is_response_enc={} leases={}",
            hex::encode(&exit_payload.request_id[..8]),
            hex::encode(&recipient_pubkey[..8]),
            exit_payload.response_enc_pubkey != [0u8; 32],
            exit_payload.lease_set.leases.len(),
        );
        let encrypted_response = tunnelcraft_crypto::encrypt_for_recipient(
            recipient_pubkey,
            &self.encryption_keypair.secret_key_bytes(),
            response_data,
        ).map_err(|e| ExitError::InvalidRequest(format!("Response encryption failed: {}", e)))?;

        // Prepend original length (4-byte LE u32) so client can strip erasure padding
        let original_len = encrypted_response.len() as u32;
        let mut framed = Vec::with_capacity(4 + encrypted_response.len());
        framed.extend_from_slice(&original_len.to_le_bytes());
        framed.extend_from_slice(&encrypted_response);

        // Chunk and erasure code
        let chunks = chunk_and_encode(&framed)
            .map_err(|e| ExitError::ErasureDecodeError(e.to_string()))?;

        let total_chunks = chunks.len() as u16;
        // Use request_id as assembly_id so the client can match
        // response shards to its pending request map.
        let assembly_id = exit_payload.request_id;

        let leases = &exit_payload.lease_set.leases;
        let mut shard_pairs = Vec::with_capacity(chunks.len() * tunnelcraft_erasure::TOTAL_SHARDS);
        let mut shard_counter: usize = 0;

        for (chunk_index, shard_payloads) in chunks {
            let total_shards_in_chunk = shard_payloads.len() as u8;

            for (i, payload) in shard_payloads.into_iter().enumerate() {
                // For each shard, build a routing tag encrypted for the client
                // Response routing tags don't need pool_pubkey (client doesn't enforce limits)
                let routing_tag = encrypt_routing_tag(
                    recipient_pubkey,
                    &assembly_id,
                    i as u8,
                    total_shards_in_chunk,
                    chunk_index,
                    total_chunks,
                    &[0u8; 32],
                ).map_err(|e| ExitError::InvalidRequest(
                    format!("routing_tag encrypt failed: {}", e),
                ))?;

                // Round-robin this shard's gateway across all leases
                let lease = if !leases.is_empty() {
                    Some(&leases[shard_counter % leases.len()])
                } else {
                    None
                };
                shard_counter += 1;

                let (header, ephemeral, gateway) = if let Some(lease) = lease {
                    // Direct mode: tunnel_id is all zeros — skip onion header,
                    // send response shards directly to client's peer_id.
                    if lease.tunnel_id == [0u8; 32] {
                        (vec![], [0u8; 32], Some(lease.gateway_peer_id.clone()))
                    } else {
                        let gateway_pubkey = lease.gateway_encryption_pubkey;
                        let shard_id = {
                            let mut hasher = Sha256::new();
                            hasher.update(exit_payload.request_id);
                            hasher.update(b"response");
                            hasher.update(chunk_index.to_be_bytes());
                            hasher.update([i as u8]);
                            hasher.update(gateway_pubkey);
                            let hash = hasher.finalize();
                            let mut id: Id = [0u8; 32];
                            id.copy_from_slice(&hash);
                            id
                        };

                        let settlement = vec![OnionSettlement {
                            shard_id,
                            payload_size: payload.len() as u32,
                            pool_pubkey: exit_payload.user_pubkey,
                        }];

                        // Single-hop onion to this shard's gateway with tunnel_id
                        let (h, e) = build_onion_header(
                            &[(&lease.gateway_peer_id, &lease.gateway_encryption_pubkey)],
                            (&lease.gateway_peer_id, &lease.gateway_encryption_pubkey),
                            &settlement,
                            Some(&lease.tunnel_id),
                        ).map_err(|e| ExitError::InvalidRequest(
                            format!("Onion header build failed: {}", e),
                        ))?;
                        (h, e, Some(lease.gateway_peer_id.clone()))
                    }
                } else {
                    // No lease set — fallback (empty header, no gateway)
                    (vec![], [0u8; 32], None)
                };

                shard_pairs.push((
                    Shard::new(ephemeral, header, payload, routing_tag, 0, 0),
                    gateway,
                ));
            }
        }

        debug!(
            "Created {} response shards ({} chunks) for request {} across {} gateways",
            shard_pairs.len(),
            total_chunks,
            hex::encode(&exit_payload.request_id[..8]),
            leases.len(),
        );

        Ok(shard_pairs)
    }

    /// Get the number of pending assemblies
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Clear stale pending assemblies, tunnel sessions, and inactive user trackers
    pub fn clear_stale(&mut self, max_age: Duration) {
        let now = Instant::now();

        // Evict stale pending assemblies, decrementing per-user counters
        let before = self.pending.len();
        let stale_keys: Vec<Id> = self.pending.iter()
            .filter(|(_, asm)| now.duration_since(asm.created_at) >= max_age)
            .map(|(id, _)| *id)
            .collect();

        for key in &stale_keys {
            if let Some(asm) = self.pending.remove(key) {
                if let Some(tracker) = self.user_tracking.get_mut(&asm.pool_pubkey) {
                    tracker.pending_assemblies = tracker.pending_assemblies.saturating_sub(1);
                }
            }
        }

        let removed = before - self.pending.len();
        if removed > 0 {
            warn!("Cleared {} stale pending assemblies", removed);
        }

        // Evict stale tunnel sessions, decrementing per-user concurrent_tunnels
        let evicted_tunnel_owners = self.tunnel_handler.clear_stale(max_age);
        for owner in evicted_tunnel_owners {
            if let Some(tracker) = self.user_tracking.get_mut(&owner) {
                tracker.concurrent_tunnels = tracker.concurrent_tunnels.saturating_sub(1);
            }
        }

        // Clean up stale user trackers (no activity for 5 minutes)
        let tracker_timeout = Duration::from_secs(300);
        self.user_tracking.retain(|_, tracker| {
            now.duration_since(tracker.last_activity) < tracker_timeout
        });
    }

    /// Get the number of active tunnel sessions
    pub fn tunnel_session_count(&self) -> usize {
        self.tunnel_handler.session_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = ExitConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.blocked_domains.contains(&"localhost".to_string()));
    }

    #[tokio::test]
    async fn test_blocked_domain_check() {
        let config = ExitConfig::default();
        let handler = ExitHandler::new(config, [0u8; 32], [0u8; 32]).unwrap();

        assert!(handler.check_blocked("http://localhost:8080/api").await.is_err());
        assert!(handler.check_blocked("http://127.0.0.1/test").await.is_err());
        assert!(handler.check_blocked("https://example.com/api").await.is_ok());
    }

    #[test]
    fn test_handler_creation() {
        let handler = ExitHandler::new(ExitConfig::default(), [0u8; 32], [0u8; 32]).unwrap();
        assert_eq!(handler.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_blocked_localhost_variants() {
        let config = ExitConfig::default();
        let handler = ExitHandler::new(config, [0u8; 32], [0u8; 32]).unwrap();

        assert!(handler.check_blocked("http://localhost").await.is_err());
        assert!(handler.check_blocked("http://localhost:3000").await.is_err());
        assert!(handler.check_blocked("https://localhost/api").await.is_err());
        assert!(handler.check_blocked("http://127.0.0.1").await.is_err());
        assert!(handler.check_blocked("http://0.0.0.0:9000").await.is_err());
    }

    #[tokio::test]
    async fn test_custom_blocked_domains() {
        let config = ExitConfig {
            blocked_domains: vec![
                "malware.com".to_string(),
                "phishing.net".to_string(),
            ],
            allow_private_ips: true, // test blocklist only, not SSRF
            ..Default::default()
        };
        let handler = ExitHandler::new(config, [0u8; 32], [0u8; 32]).unwrap();

        assert!(handler.check_blocked("http://malware.com").await.is_err());
        assert!(handler.check_blocked("https://phishing.net/login").await.is_err());
        assert!(handler.check_blocked("https://safe.org").await.is_ok());
        assert!(handler.check_blocked("http://localhost").await.is_ok());
    }

    #[test]
    fn test_handler_with_keypair() {
        let keypair = SigningKeypair::generate();
        let pubkey = keypair.public_key_bytes();
        let handler = ExitHandler::with_keypair(ExitConfig::default(), keypair).unwrap();
        assert_eq!(handler.keypair.public_key_bytes(), pubkey);
        assert_eq!(handler.pending_count(), 0);
    }

    #[test]
    fn test_encryption_pubkey() {
        let handler = ExitHandler::new(ExitConfig::default(), [0u8; 32], [0u8; 32]).unwrap();
        let enc_pub = handler.encryption_pubkey();
        // Should be a valid non-zero X25519 pubkey
        assert_ne!(enc_pub, [0u8; 32]);
    }

    #[test]
    fn test_clear_stale_removes_old_entries() {
        let keypair = SigningKeypair::generate();
        let mut handler = ExitHandler::with_keypair(ExitConfig::default(), keypair).unwrap();

        handler.pending.insert([1u8; 32], PendingAssembly {
            shards: HashMap::new(),
            total_chunks: 1,
            total_shards: 5,
            created_at: Instant::now() - Duration::from_secs(120),
            pool_pubkey: [0u8; 32],
        });
        handler.pending.insert([2u8; 32], PendingAssembly {
            shards: HashMap::new(),
            total_chunks: 1,
            total_shards: 5,
            created_at: Instant::now(),
            pool_pubkey: [0u8; 32],
        });

        assert_eq!(handler.pending_count(), 2);
        handler.clear_stale(Duration::from_secs(60));
        assert_eq!(handler.pending_count(), 1);
        assert!(handler.pending.contains_key(&[2u8; 32]));
    }

    #[tokio::test]
    async fn test_empty_blocked_list() {
        let config = ExitConfig {
            blocked_domains: vec![],
            allow_private_ips: true, // allow private IPs so only blocklist is tested
            ..Default::default()
        };
        let handler = ExitHandler::new(config, [0u8; 32], [0u8; 32]).unwrap();

        assert!(handler.check_blocked("http://localhost").await.is_ok());
        assert!(handler.check_blocked("http://127.0.0.1").await.is_ok());
    }

    #[test]
    fn test_is_private_ip() {
        use std::net::{Ipv4Addr, Ipv6Addr};

        // Loopback
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        // RFC 1918
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        // Link-local / metadata
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
        // CGNAT
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(100, 127, 255, 254))));
        // Unspecified
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));

        // Public IPs
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1))));
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1))));

        // IPv6
        assert!(is_private_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_private_ip(IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
        assert!(is_private_ip(IpAddr::V6("fc00::1".parse().unwrap())));
        assert!(is_private_ip(IpAddr::V6("fe80::1".parse().unwrap())));
        assert!(!is_private_ip(IpAddr::V6("2001:db8::1".parse().unwrap())));
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(extract_host("http://example.com/path"), "example.com");
        assert_eq!(extract_host("https://example.com:443/path"), "example.com");
        assert_eq!(extract_host("http://127.0.0.1:8080/api"), "127.0.0.1");
        assert_eq!(extract_host("example.com:443"), "example.com");
        assert_eq!(extract_host("example.com"), "example.com");
    }

    #[tokio::test]
    async fn test_ssrf_blocks_private_ips() {
        let config = ExitConfig {
            blocked_domains: vec![],
            allow_private_ips: false,
            ..Default::default()
        };
        let handler = ExitHandler::new(config, [0u8; 32], [0u8; 32]).unwrap();

        // Direct IP addresses should be blocked
        assert!(handler.check_blocked("http://127.0.0.1/api").await.is_err());
        assert!(handler.check_blocked("http://10.0.0.1/api").await.is_err());
        assert!(handler.check_blocked("http://192.168.1.1/api").await.is_err());
        assert!(handler.check_blocked("http://169.254.169.254/metadata").await.is_err());
    }

    #[tokio::test]
    async fn test_ssrf_allows_when_configured() {
        let config = ExitConfig {
            blocked_domains: vec![],
            allow_private_ips: true,
            ..Default::default()
        };
        let handler = ExitHandler::new(config, [0u8; 32], [0u8; 32]).unwrap();

        // With allow_private_ips=true, private IPs should pass
        assert!(handler.check_blocked("http://127.0.0.1/api").await.is_ok());
        assert!(handler.check_blocked("http://10.0.0.1/api").await.is_ok());
    }
}

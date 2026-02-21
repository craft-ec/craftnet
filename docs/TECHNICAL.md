# CraftNet: Technical Specification

## Technology Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| Language | Rust 1.75+ | Core implementation |
| Runtime | Tokio | Async runtime |
| P2P | libp2p | Discovery, NAT traversal, gossip |
| Erasure | reed-solomon-erasure | Shard encoding (5/3, 3KB chunks) |
| Crypto | dalek ecosystem (ed25519-dalek, x25519-dalek, chacha20poly1305) | Encryption, signatures |
| Settlement | Solana + Anchor | Subscriptions, pools, rewards |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                       CLIENT                                 │
│  • SOCKS5 proxy (localhost:1080)                             │
│  • Tunnel shard builder (L4 TCP)                             │
│  • HTTP request builder (L7)                                 │
│  • Subscription management                                   │
│  • Erasure encoding (3KB chunks → 5 shards)                  │
│  • Exit selection + hop mode setting                         │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────┼─────────────────────────────────┐
│                      NETWORK                                 │
│  ┌────────────────────────────────────────────────────┐     │
│  │                 libp2p Kademlia DHT                 │     │
│  │  • Peer discovery                                   │     │
│  │  • Exit lookup                                      │     │
│  │  • Subscription gossip (gossipsub)                  │     │
│  │  • NAT traversal (circuit relay)                    │     │
│  └────────────────────────────────────────────────────┘     │
│                           │                                  │
│  ┌────────────────────────────────────────────────────┐     │
│  │                    Relay                            │     │
│  │  • Best-effort forwarding (decrement hops)          │     │
│  │  • sender_pubkey stamping                           │     │
│  │  • Destination verification (response path)         │     │
│  │  • Request-origin caching                           │     │
│  │  • Subscription check (gossip cache)                │     │
│  │  • ForwardReceipt collection                        │     │
│  └────────────────────────────────────────────────────┘     │
│                           │                                  │
│  ┌────────────────────────────────────────────────────┐     │
│  │                    Exit                             │     │
│  │  • Request reconstruction (multi-chunk)             │     │
│  │  • Mode dispatch: tunnel (0x01) or HTTP (0x00)      │     │
│  │  • TCP tunnel handler (session pool)                │     │
│  │  • HTTP fetch (legacy)                              │     │
│  │  • Response shard creation                          │     │
│  └────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
                            │
┌───────────────────────────┼─────────────────────────────────┐
│                     SETTLEMENT                               │
│  ┌────────────────────────────────────────────────────┐     │
│  │                    Solana                           │     │
│  │  • SubscriptionPDA (tier, expiry)                   │     │
│  │  • UserPoolPDA (per-user reward pool)               │     │
│  │  • PoolReceiptsPDA (receipt counts per relay)       │     │
│  │  • NodeAccount (relay earnings)                     │     │
│  └────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

---

## Core Types

### Shard

```rust
pub struct Shard {
    pub shard_id: [u8; 32],
    pub request_id: [u8; 32],
    pub user_pubkey: PublicKey,         // User who originated request
    pub destination: PublicKey,         // Exit for request, User for response
    pub user_proof: [u8; 32],          // SHA256(request_id || user_pubkey || sig)
    pub hops_remaining: u8,            // Decremented per relay
    pub total_hops: u8,                // Never decremented (for response hop count)
    pub sender_pubkey: PublicKey,       // Last relay's identity (stamped before forwarding)
    pub payload: Vec<u8>,
    pub shard_type: ShardType,         // Request or Response
    pub shard_index: u8,               // Index within chunk (0-4)
    pub total_shards: u8,              // Always 5
    pub chunk_index: u16,              // Which 3KB chunk
    pub total_chunks: u16,             // Total chunks in request/response
}
```

No ChainEntry vec — `sender_pubkey` tracks the last hop directly.
`user_proof` binds shards to user's settlement pool.
`chunk_index`/`total_chunks` enable multi-chunk reassembly for payloads >3KB.

### ForwardReceipt

```rust
pub struct ForwardReceipt {
    pub request_id: [u8; 32],
    pub shard_id: [u8; 32],           // Distinguishes request vs response shards
    pub sender_pubkey: [u8; 32],       // Relay that forwarded (anti-Sybil)
    pub receiver_pubkey: [u8; 32],     // Node that signs this receipt
    pub user_proof: [u8; 32],          // Binding to user's pool
    pub payload_size: u32,             // Bandwidth-weighted settlement
    pub epoch: u64,                    // Prevents cross-epoch replay
    pub timestamp: u64,                // Unix seconds
    pub signature: [u8; 64],           // Receiver's ed25519 signature
}
```

The only settlement primitive. Proves a node received a shard.
Deduped on-chain by (request_id, shard_index, receiver_pubkey).
Bandwidth-weighted via `payload_size`.

### TunnelMetadata

```rust
pub const PAYLOAD_MODE_HTTP: u8 = 0x00;
pub const PAYLOAD_MODE_TUNNEL: u8 = 0x01;

pub struct TunnelMetadata {
    pub host: String,           // "youtube.com"
    pub port: u16,              // 443
    pub session_id: [u8; 32],   // Shared across all bursts for one SOCKS5 connection
    pub is_close: bool,         // Signals TCP connection teardown
}
```

### HopMode

```rust
pub enum HopMode {
    Direct,   // 0 min relays
    Light,    // 1 min relay
    Standard, // 2 min relays
    Paranoid, // 3 min relays
}

impl HopMode {
    pub fn min_relays(&self) -> u8 { ... }
}
```

### Type Aliases

```rust
pub type Id = [u8; 32];
pub type PublicKey = [u8; 32];
pub type Signature = [u8; 64];
```

---

## Relay Logic

### Request Handling (Best-Effort)

```rust
impl RelayHandler {
    pub fn handle_request(&mut self, mut shard: Shard) -> Result<Option<Shard>> {
        // 1. Cache request origin
        self.cache.insert(shard.request_id, shard.user_pubkey);

        // 2. Stamp sender_pubkey
        shard.sender_pubkey = self.keypair.public_key_bytes();

        // 3. Decrement minimum relay counter
        shard.decrement_hops();

        // 4. Return shard for forwarding (best-effort — never drops)
        // Network layer picks next hop:
        //   hops_remaining > 0 → forward to another relay
        //   hops_remaining = 0 → forward toward exit (direct or via peers)
        Ok(Some(shard))
    }
}
```

### Response Handling (Trustless Verification)

```rust
impl RelayHandler {
    pub fn handle_response(&mut self, mut shard: Shard) -> Result<Option<Shard>> {
        // 1. CRITICAL: Verify destination matches cached origin
        if let Some(expected_user) = self.cache.get(&shard.request_id) {
            if shard.destination != *expected_user {
                // EXIT TRIED TO REDIRECT — DROP
                return Err(Error::DestinationMismatch);
            }
        }

        // 2. Stamp sender_pubkey
        shard.sender_pubkey = self.keypair.public_key_bytes();

        // 3. Decrement and return for forwarding
        shard.decrement_hops();
        Ok(Some(shard))
    }
}
```

---

## Exit Logic

### Dual-Mode Dispatch

```rust
impl ExitHandler {
    pub async fn process_reconstructed(
        &mut self,
        request_id: Id,
        request_data: Vec<u8>,
        pending: &PendingRequest,
    ) -> Result<Vec<Shard>> {
        // Check first byte for mode
        if !request_data.is_empty() && request_data[0] == PAYLOAD_MODE_TUNNEL {
            // TCP Tunnel Mode (L4)
            let metadata_len = u32::from_be_bytes(
                request_data[1..5].try_into().unwrap()
            ) as usize;
            let metadata = TunnelMetadata::from_bytes(
                &request_data[5..5 + metadata_len]
            )?;
            let tcp_data = &request_data[5 + metadata_len..];

            self.tunnel_handler.process_tunnel_data(
                request_id, metadata, tcp_data,
                pending.user_pubkey, pending.user_proof, pending.total_hops,
            ).await
        } else {
            // HTTP Mode (L7)
            let http_request = HttpRequest::from_bytes(&request_data)?;
            self.execute_http(request_id, http_request, pending).await
        }
    }
}
```

### Tunnel Handler (TCP Session Pool)

```rust
pub struct TunnelHandler {
    sessions: HashMap<Id, TcpSession>,  // session_id → active TCP connection
    keypair: SigningKeypair,
}

struct TcpSession {
    stream: TcpStream,
    user_pubkey: PublicKey,
    user_proof: Id,
    total_hops: u8,
    last_activity: Instant,
}
```

Methods:
- `process_tunnel_data()` — get/create TCP session, write data, read response, create response shards
- `clear_stale(max_age)` — remove sessions idle > timeout

---

## Erasure Coding

### Constants

```rust
pub const DATA_SHARDS: usize = 3;
pub const PARITY_SHARDS: usize = 2;
pub const TOTAL_SHARDS: usize = 5;  // 3 + 2
```

### Chunked Encoding

```rust
/// Chunk data into 3KB pieces, encode each chunk into 5 shard payloads.
/// Returns Vec<(chunk_index, Vec<shard_payload>)>
pub fn chunk_and_encode(data: &[u8]) -> Result<Vec<(u16, Vec<Vec<u8>>)>>;

/// Reassemble from shards: group by chunk_index, decode each chunk,
/// concatenate in order, trim to original length.
pub fn reassemble(shards: &[(u16, Vec<Option<Vec<u8>>>)], original_len: usize) -> Result<Vec<u8>>;
```

---

## Settlement Contracts (Solana)

### Account Structures

```rust
/// PDA: ["subscription", user_pubkey]
pub struct SubscriptionPDA {
    pub user_pubkey: [u8; 32],
    pub tier: u8,              // 0=Basic, 1=Standard, 2=Premium
    pub expires_at: i64,
    pub bump: u8,
}

/// PDA: ["pool", user_pubkey, cycle_id]
pub struct UserPoolPDA {
    pub user_pubkey: [u8; 32],
    pub cycle_id: u64,
    pub balance: u64,          // Pool balance (subscription payment)
    pub total_bandwidth: u64,  // Total bandwidth submitted against this pool
    pub claimed: bool,
    pub bump: u8,
}

/// PDA: ["receipts", pool_pda, relay_pubkey]
pub struct PoolReceiptsPDA {
    pub pool: [u8; 32],
    pub relay_pubkey: [u8; 32],
    pub bandwidth_forwarded: u64,  // Sum of payload_size from receipts
    pub claimed: bool,
    pub bump: u8,
}

/// PDA: ["node", node_pubkey]
pub struct NodeAccount {
    pub node_pubkey: [u8; 32],
    pub total_earned: u64,
    pub bump: u8,
}
```

### Instructions

```rust
#[program]
pub mod craftnet {
    /// Subscribe — create/renew subscription + fund user pool
    pub fn subscribe(ctx: Context<Subscribe>, tier: u8, payment_amount: u64) -> Result<()>;

    /// Submit receipts — relay submits ForwardReceipts against user's pool
    /// Deduped by (request_id, shard_index, receiver_pubkey)
    /// Increments PoolReceiptsPDA.bandwidth_forwarded by sum(payload_size)
    pub fn submit_receipts(ctx: Context<SubmitReceipts>, receipts: Vec<ForwardReceipt>) -> Result<()>;

    /// Claim rewards — relay claims bandwidth-weighted share of user's pool
    /// payout = (relay_bandwidth / total_bandwidth) * pool_balance
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()>;

    /// Withdraw accumulated earnings
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()>;
}
```

### Protection Model

| Mechanism | Protects against | How |
|-----------|-----------------|-----|
| Per-user pool | Pool inflation attack | Abuse only dilutes abuser's own pool |
| Receipt dedup | Relay double-claiming | Same receipt can't be submitted twice |
| Receipt signature | Forged receipts | ed25519 — can't forge without private key |
| Bandwidth weighting | Over-extraction | Relay gets share proportional to bandwidth forwarded |
| Epoch field | Cross-epoch replay | ForwardReceipt.epoch prevents replay |
| user_proof | Receipt theft | Binds receipts to specific user's pool |
| Gossip + random audit | Fake subscriptions | Spot-check catches fakers |
| Priority queuing | Free-riding | Non-subscribers get best-effort only |

---

## Wire Format

### Shard Packet

```
┌────────────────────────────────────────────────────────────┐
│  HEADER                                                    │
├────────────────────────────────────────────────────────────┤
│  magic:          4 bytes   [0x54, 0x43, 0x53, 0x48]        │
│  version:        1 byte    (1)                             │
│  type:           1 byte    (0=request, 1=response)         │
├────────────────────────────────────────────────────────────┤
│  shard_id:       32 bytes                                  │
│  request_id:     32 bytes                                  │
│  user_pubkey:    32 bytes                                  │
│  destination:    32 bytes                                  │
│  user_proof:     32 bytes                                  │
│  sender_pubkey:  32 bytes                                  │
│  hops_remaining: 1 byte                                    │
│  total_hops:     1 byte                                    │
│  shard_index:    1 byte                                    │
│  total_shards:   1 byte                                    │
│  chunk_index:    2 bytes (u16 BE)                          │
│  total_chunks:   2 bytes (u16 BE)                          │
├────────────────────────────────────────────────────────────┤
│  payload_len:    4 bytes                                   │
│  payload:        variable                                  │
└────────────────────────────────────────────────────────────┘
```

### Tunnel Payload Format

```
┌────────────────────────────────────────────────────────────┐
│  mode:           1 byte    (0x01 = tunnel)                 │
│  metadata_len:   4 bytes   (u32 BE)                        │
│  metadata:       variable  (bincode-encoded TunnelMetadata) │
│  tcp_data:       variable  (raw TCP bytes)                 │
└────────────────────────────────────────────────────────────┘
```

### ShardResponse

```
┌────────────────────────────────────────────────────────────┐
│  type: 1 byte                                              │
│    0 = Accepted (no receipt)                                │
│    1 = Rejected (+ reason string)                          │
│    2 = Accepted with ForwardReceipt (bincode serialized)   │
└────────────────────────────────────────────────────────────┘
```

---

## Crate Structure

```
craftnet/
├── Cargo.toml
├── crates/
│   ├── core/              # Types, traits, errors
│   │   ├── shard.rs       # Shard struct + serialization
│   │   ├── types.rs       # Id, PublicKey, HopMode, ForwardReceipt, ChainEntry
│   │   ├── tunnel.rs      # TunnelMetadata, PAYLOAD_MODE_* constants
│   │   ├── geo.rs         # ExitRegion, country codes
│   │   └── error.rs       # Error types
│   │
│   ├── crypto/            # Encryption, signatures
│   │   ├── keys.rs        # SigningKeypair, key generation
│   │   └── sign.rs        # sign_data, verify_data
│   │
│   ├── erasure/           # Reed-Solomon (5/3, 3KB chunks)
│   │   ├── lib.rs         # ErasureCoder, constants
│   │   └── chunker.rs     # chunk_and_encode, reassemble
│   │
│   ├── network/           # libp2p integration
│   │   ├── swarm.rs       # Swarm setup, behavior
│   │   └── protocol.rs    # Wire protocol, shard transport
│   │
│   ├── relay/             # Relay logic
│   │   └── handler.rs     # RelayHandler (request/response handling, destination verification)
│   │
│   ├── exit/              # Exit node
│   │   ├── handler.rs     # ExitHandler (reconstruction, mode dispatch)
│   │   └── tunnel_handler.rs  # TunnelHandler (TCP session pool)
│   │
│   ├── settlement/        # Solana client
│   │   └── client.rs      # Settlement client (mock + live modes)
│   │
│   ├── client/            # User client
│   │   ├── node.rs        # CraftNetNode (event loop, shard routing)
│   │   ├── request.rs     # RequestBuilder (HTTP mode)
│   │   ├── tunnel.rs      # build_tunnel_shards() (tunnel mode)
│   │   └── socks5.rs      # SOCKS5 proxy server (RFC 1928)
│   │
│   ├── daemon/            # Background service
│   │   ├── service.rs     # IPC server (JSON-RPC)
│   │   └── windows_pipe.rs # Windows named pipe support
│   │
│   └── uniffi/            # Mobile FFI bindings
│
├── programs/              # Solana programs
│   └── craftnet-settlement/
│
└── apps/
    ├── cli/               # CLI application
    ├── desktop/           # Electron app
    └── mobile/            # React Native app
        ├── ios/           # Swift Network Extension
        └── android/       # Kotlin VpnService
```

---

## Security Properties

### Cryptographic Guarantees

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   SUBSCRIPTION VALID                                         │
│   Proof: On-chain SubscriptionPDA                            │
│   Verification: Gossip cache + random audit                  │
│                                                              │
│   WORK DONE                                                  │
│   Proof: ForwardReceipt signed by next-hop receiver          │
│   Weighted: payload_size for bandwidth-based settlement      │
│                                                              │
│   SETTLEMENT BOUND                                           │
│   Proof: user_proof = SHA256(request_id || pubkey || sig)     │
│   Binds receipts to specific user's pool PDA                 │
│                                                              │
│   NO DOUBLE CLAIM                                            │
│   Proof: Receipts deduped by (request_id, shard_index,       │
│          receiver_pubkey) — PDA exists = duplicate            │
│                                                              │
│   NO CROSS-EPOCH REPLAY                                      │
│   Proof: ForwardReceipt.epoch checked on-chain               │
│                                                              │
│   NO REDIRECT POSSIBLE                                       │
│   Proof: Relays verify destination == origin                 │
│                                                              │
│   NO POOL INFLATION                                          │
│   Proof: Per-user pool — abuse dilutes abuser only           │
│                                                              │
│   TRUST REQUIRED: None                                       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

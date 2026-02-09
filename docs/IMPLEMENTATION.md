# TunnelCraft: Implementation Roadmap

## Overview

Implementation roadmap for a decentralized P2P VPN with:
- libp2p for discovery and NAT traversal
- Best-effort routing with minimum relay count
- SOCKS5 proxy + TCP tunnel mode (L4)
- ForwardReceipt-based settlement
- Trustless relay verification

---

## Phase 1: Core Types - COMPLETE

### Deliverables

```rust
pub struct Shard {
    pub shard_id: Id,
    pub request_id: Id,
    pub user_pubkey: PublicKey,
    pub destination: PublicKey,
    pub user_proof: Id,            // Settlement binding
    pub hops_remaining: u8,
    pub total_hops: u8,
    pub sender_pubkey: PublicKey,   // Last relay identity
    pub payload: Vec<u8>,
    pub shard_type: ShardType,
    pub shard_index: u8,
    pub total_shards: u8,
    pub chunk_index: u16,
    pub total_chunks: u16,
}

pub struct ForwardReceipt {
    pub request_id: Id,
    pub shard_id: Id,
    pub sender_pubkey: PublicKey,
    pub receiver_pubkey: PublicKey,
    pub user_proof: Id,
    pub payload_size: u32,
    pub epoch: u64,
    pub timestamp: u64,
    pub signature: Signature,
}

pub struct TunnelMetadata {
    pub host: String,
    pub port: u16,
    pub session_id: Id,
    pub is_close: bool,
}
```

**Status**: Complete. All types implemented with serialization/deserialization and tests.

---

## Phase 2: Erasure Coding - COMPLETE

### Deliverables

```rust
pub const DATA_SHARDS: usize = 3;
pub const PARITY_SHARDS: usize = 2;
pub const TOTAL_SHARDS: usize = 5;

// Chunked encoding: 3KB chunks → 5 shard payloads each
pub fn chunk_and_encode(data: &[u8]) -> Result<Vec<(u16, Vec<Vec<u8>>)>>;
pub fn reassemble(shards: &[(u16, Vec<Option<Vec<u8>>>)], original_len: usize) -> Result<Vec<u8>>;
```

**Status**: Complete. Reed-Solomon 5/3 with 3KB chunked encoding.

---

## Phase 3: Cryptography - COMPLETE

### Deliverables

```rust
pub struct SigningKeypair { ... }

impl SigningKeypair {
    pub fn generate() -> Self;
    pub fn public_key_bytes(&self) -> PublicKey;
}

pub fn sign_data(keypair: &SigningKeypair, data: &[u8]) -> Signature;
pub fn verify_data(pubkey: &PublicKey, data: &[u8], signature: &Signature) -> bool;
```

**Status**: Complete. Ed25519 signing, X25519 key exchange, ChaCha20Poly1305 encryption.

---

## Phase 4: Relay Logic - COMPLETE

### Deliverables

```rust
impl RelayHandler {
    // Cache request origins, stamp sender_pubkey, decrement hops
    pub fn handle_request(&mut self, shard: Shard) -> Result<Option<Shard>>;

    // CRITICAL: Verify destination matches origin, then forward
    pub fn handle_response(&mut self, shard: Shard) -> Result<Option<Shard>>;
}
```

**Status**: Complete. Best-effort routing with minimum relay count. Trustless destination verification.

---

## Phase 5: Networking - COMPLETE

### Deliverables

```rust
// libp2p integration with Kademlia DHT, gossipsub, circuit relay
pub struct NetworkManager { ... }

impl NetworkManager {
    pub async fn find_exits(&self) -> Vec<ExitInfo>;
    pub async fn find_peers(&self) -> Vec<PeerInfo>;
    pub async fn announce(&self, pubkey: PublicKey);
}
```

**Status**: Complete. libp2p with Kademlia, gossipsub, NAT traversal via circuit relay.

---

## Phase 6: Exit Node - COMPLETE

### Deliverables

```rust
impl ExitHandler {
    // Dual-mode: TCP tunnel (0x01) or HTTP fetch (0x00)
    pub async fn process_shard(&mut self, shard: Shard) -> Result<Option<Vec<Shard>>>;
}

pub struct TunnelHandler {
    sessions: HashMap<Id, TcpSession>,
}
```

**Status**: Complete. HTTP fetch + TCP tunnel handler with session pool.

---

## Phase 7: Settlement - PARTIAL

### Deliverables

```rust
#[program]
pub mod tunnelcraft {
    pub fn subscribe(tier: u8, payment: u64) -> Result<()>;
    pub fn submit_receipts(receipts: Vec<ForwardReceipt>) -> Result<()>;
    pub fn claim_rewards() -> Result<()>;
    pub fn withdraw(amount: u64) -> Result<()>;
}
```

**Status**: Settlement client implemented (mock + Photon live mode). On-chain program structure defined. Bandwidth-weighted settlement with user_proof binding.

---

## Phase 8: Client SDK + SOCKS5 - COMPLETE

### Deliverables

```rust
pub struct TunnelCraftNode { ... }

impl TunnelCraftNode {
    pub async fn connect(&mut self) -> Result<()>;
    pub async fn request(&mut self, url: &str) -> Result<TunnelResponse>;
    pub async fn send_tunnel_burst(&mut self, metadata: TunnelMetadata, data: &[u8]) -> Result<...>;
}

pub struct RequestBuilder { ... }      // HTTP mode
pub fn build_tunnel_shards(...);       // Tunnel mode

pub struct Socks5Server { ... }        // RFC 1928 SOCKS5 proxy
```

**Status**: Complete. Full client SDK with SOCKS5 proxy, tunnel shard builder, and HTTP request builder.

---

## Phase 9: Applications - COMPLETE (Structure)

### CLI
```bash
tunnelcraft connect [--hops 2]
tunnelcraft status
tunnelcraft balance
```

### Desktop (Electron)
- System tray, connect/disconnect, privacy level selector, network stats, request panel

### Mobile (React Native)
- iOS: Network Extension + TUN interface
- Android: VpnService + TUN interface
- tun2socks → SOCKS5 proxy (localhost:1080)

**Status**: UI complete for all platforms. Native bridges implemented. Integration testing pending.

---

## Phase 10: Hardening & Launch - PENDING

### Security
- External audit
- Penetration testing
- Bug bounty program

### Performance
- Profiling
- Optimization
- Load testing

### Launch
- Bootstrap nodes (50+)
- Testnet
- Mainnet

---

## Future: Anonymity Layer

### Phase A: Topology Gossip
- Relays gossip connected peer lists via gossipsub
- Client builds local topology graph
- Event-driven updates (connect/disconnect/heartbeat timeout)

### Phase B: Onion Routing
- Client picks exact paths from topology graph
- Layered onion encryption (Sphinx or simplified variant)
- Each relay decrypts one layer, sees only next hop
- Fixed hop count (not best-effort)

### Phase C: Lease Sets
- Client publishes anonymous entry points (gateway + tunnel_id)
- Exit picks gateway from lease set, builds onion path to it
- Connections ARE the lease set (no pre-built tunnels)

### Phase D: Blind Subscription Tokens
- Blind-signed tokens prove valid subscription without revealing identity
- `subscription_id` rotates per epoch (no cross-epoch tracking)
- Every relay verifies blind signature
- Settlement uses `subscription_id` → pool commitment

### Phase E: Participatory Relay
- Free users earn credits by forwarding shards
- `net_balance = shards_forwarded - shards_consumed`
- Net positive → no throttle (Basic-tier treatment)
- Gateway-local accounting (no global ledger)

---

## Current Status Summary

| Phase | Status | Key Files |
|-------|--------|-----------|
| 1. Core Types | Complete | `crates/core/src/{shard,types,tunnel}.rs` |
| 2. Erasure Coding | Complete | `crates/erasure/src/{lib,chunker}.rs` |
| 3. Cryptography | Complete | `crates/crypto/src/{keys,sign}.rs` |
| 4. Relay Logic | Complete | `crates/relay/src/handler.rs` |
| 5. Networking | Complete | `crates/network/src/{swarm,protocol}.rs` |
| 6. Exit Node | Complete | `crates/exit/src/{handler,tunnel_handler}.rs` |
| 7. Settlement | Partial | `crates/settlement/src/client.rs`, `programs/` |
| 8. Client + SOCKS5 | Complete | `crates/client/src/{node,request,tunnel,socks5}.rs` |
| 9. Applications | Structure complete | `apps/{cli,desktop,mobile}/` |
| 10. Hardening | Pending | — |

# TunnelCraft: Implementation Plan

## Overview

10-month implementation plan for trustless P2P VPN with:
- DHT for discovery
- Random routing (network decides path)
- Chain signatures for proof
- Trustless verification (relays check destination)
- Two-phase settlement

---

## Phase 1: Core Types (Weeks 1-3)

### Goal
Define data structures and traits.

### Deliverables

```rust
// Shard structure
pub struct Shard { ... }

// Chain entry
pub struct ChainEntry {
    pub pubkey: PublicKey,
    pub signature: Signature,
}

// Relay cache for trustless verification
pub struct RelayCache {
    requests: HashMap<[u8; 32], PublicKey>,
}
```

### Tasks

```
- [ ] Shard struct
- [ ] ChainEntry struct
- [ ] RelayCache struct
- [ ] Serialization (bincode)
- [ ] Unit tests
```

---

## Phase 2: Erasure Coding (Weeks 4-5)

### Goal
Reed-Solomon 5/3 encoding.

### Deliverables

```rust
pub struct Encoder {
    rs: ReedSolomon,
}

impl Encoder {
    pub fn encode(&self, data: &[u8]) -> Vec<Vec<u8>>;
    pub fn decode(&self, shards: &[Option<Vec<u8>>]) -> Result<Vec<u8>>;
}
```

### Tasks

```
- [ ] Encode function
- [ ] Decode with 3+ shards
- [ ] Handle large payloads (chunking)
- [ ] Unit tests
```

---

## Phase 3: Chain Signatures (Weeks 6-8)

### Goal
Signature chain accumulation and verification.

### Deliverables

```rust
impl Shard {
    pub fn add_signature(&mut self, keypair: &Keypair);
    pub fn verify_chain(&self) -> bool;
}
```

### Tasks

```
- [ ] Signature creation
- [ ] Chain accumulation
- [ ] Chain verification
- [ ] Unit tests
```

---

## Phase 4: Relay Logic (Weeks 9-12)

### Goal
Request/response handling with trustless verification.

### Deliverables

```rust
impl Relay {
    // Cache request origins
    pub async fn handle_request(&self, shard: Shard) -> Result<()>;
    
    // CRITICAL: Verify destination matches origin
    pub async fn handle_response(&self, shard: Shard) -> Result<()>;
    
    // Last hop delivery with TCP ACK
    pub async fn deliver_to_user(&self, shard: Shard) -> Result<()>;
}
```

### Key Implementation

```rust
pub async fn handle_response(&self, mut shard: Shard) -> Result<()> {
    // TRUSTLESS VERIFICATION
    if let Some(expected_user) = self.cache.get(&shard.request_id) {
        if shard.destination != *expected_user {
            // DROP - Exit tried to redirect
            return Err(Error::DestinationMismatch);
        }
    }
    
    // Continue with signing and forwarding...
}
```

### Tasks

```
- [ ] Request handling
- [ ] Origin caching
- [ ] Response handling
- [ ] Destination verification (trustless)
- [ ] Random next hop selection
- [ ] Last hop TCP ACK
- [ ] Integration tests
```

---

## Phase 5: Networking (Weeks 13-16)

### Goal
Hyperswarm DHT integration.

### Deliverables

```rust
pub struct NetworkManager {
    swarm: Hyperswarm,
}

impl NetworkManager {
    pub async fn find_exits(&self) -> Vec<ExitInfo>;
    pub async fn find_peers(&self) -> Vec<PeerInfo>;
    pub async fn announce_pubkey(&self, pubkey: PublicKey);
    pub async fn lookup_address(&self, pubkey: PublicKey) -> Address;
}
```

### Tasks

```
- [ ] Hyperswarm setup
- [ ] Exit discovery
- [ ] Peer discovery
- [ ] Pubkey announcement
- [ ] Address lookup
- [ ] NAT traversal testing
```

---

## Phase 6: Exit Node (Weeks 17-20)

### Goal
Complete exit node with settlement.

### Deliverables

```rust
impl ExitNode {
    pub async fn handle_request(&self, shards: Vec<Shard>) -> Result<()>;
    pub async fn settle_request(&self, settlement: RequestSettlement) -> Result<()>;
}
```

### Tasks

```
- [ ] Shard collection
- [ ] Request reconstruction
- [ ] credit_secret extraction
- [ ] Request settlement (Phase 1)
- [ ] HTTP fetch
- [ ] Response shard creation
- [ ] Random distribution
- [ ] Integration tests
```

---

## Phase 7: Settlement Contracts (Weeks 21-26)

### Goal
Solana smart contracts.

### Deliverables

```rust
#[program]
pub mod tunnelcraft {
    pub fn purchase_credit(...) -> Result<()>;
    pub fn settle_request(...) -> Result<()>;  // Phase 1: PENDING
    pub fn settle_response(...) -> Result<()>; // Phase 2: COMPLETE
    pub fn claim_work(...) -> Result<()>;
    pub fn withdraw(...) -> Result<()>;
}
```

### Key Verification

```rust
pub fn settle_response(...) -> Result<()> {
    // TRUSTLESS: Destination must match stored user_pubkey
    require!(
        chain_destination == request.user_pubkey,
        Error::DestinationMismatch
    );
    // ...
}
```

### Tasks

```
- [ ] Credit accounts
- [ ] Request settlement (stores user_pubkey)
- [ ] Response settlement (verifies destination)
- [ ] Points calculation
- [ ] Claim logic
- [ ] Epoch rewards
- [ ] Withdrawal
- [ ] Devnet testing
```

---

## Phase 8: Client Library (Weeks 27-30)

### Goal
User-facing SDK.

### Deliverables

```rust
pub struct Client {
    identity: IdentityManager,
    network: NetworkManager,
    encoder: Encoder,
}

impl Client {
    pub async fn request(&self, url: &str, hops: u8) -> Result<Response>;
    pub fn set_hop_count(&mut self, hops: u8);
    pub fn select_exits(&mut self, exits: Vec<ExitInfo>);
}
```

### Tasks

```
- [ ] Credit management
- [ ] One-time key generation
- [ ] Request creation
- [ ] Response assembly
- [ ] Hop count setting
- [ ] Exit selection
```

---

## Phase 9: Applications (Weeks 31-36)

### Goal
CLI and desktop apps.

### CLI

```bash
tunnelcraft connect [--hops 2]
tunnelcraft status
tunnelcraft purchase <amount>
tunnelcraft balance
```

### Desktop (Tauri)

```
Features:
- [ ] System tray
- [ ] Hop count slider
- [ ] Exit selection
- [ ] Credit management
- [ ] Connection status
```

### Node Operator

```
Features:
- [ ] Earnings display
- [ ] Points tracking
- [ ] Withdrawal
- [ ] Traffic stats
```

---

## Phase 10: Hardening & Launch (Weeks 37-42)

### Security

```
- [ ] External audit
- [ ] Penetration testing
- [ ] Bug bounty program
```

### Performance

```
- [ ] Profiling
- [ ] Optimization
- [ ] Load testing
```

### Launch

```
- [ ] Bootstrap nodes (50+)
- [ ] Testnet
- [ ] Mainnet
- [ ] Documentation
```

---

## Milestones

| Phase | Weeks | Milestone |
|-------|-------|-----------|
| 1 | 1-3 | Core types |
| 2 | 4-5 | Erasure coding |
| 3 | 6-8 | Chain signatures |
| 4 | 9-12 | Relay logic + trustless verification |
| 5 | 13-16 | Networking |
| 6 | 17-20 | Exit node |
| 7 | 21-26 | Settlement contracts |
| 8 | 27-30 | Client library |
| 9 | 31-36 | Applications |
| 10 | 37-42 | Hardening & launch |

**Total: ~10 months**

---

## Team

### Minimum: 4 people

| Role | Focus |
|------|-------|
| Protocol | Core, relay, exit |
| Blockchain | Solana contracts |
| Client | Desktop, mobile, CLI |
| DevOps | Infra, security |

---

## Key Implementation Notes

### Trustless Verification

```
MOST CRITICAL CODE:

impl Relay {
    pub async fn handle_response(&self, shard: Shard) -> Result<()> {
        // This is what makes the system trustless
        if let Some(expected) = self.cache.get(&shard.request_id) {
            if shard.destination != *expected {
                return Err(Error::DestinationMismatch);  // DROP
            }
        }
        // ...
    }
}

This single check prevents all redirect attacks.
```

### Two-Phase Settlement

```
Phase 1: Exit settles request
- Stores user_pubkey (locked)
- Status: PENDING

Phase 2: Last relay settles response  
- Verifies destination == user_pubkey
- Status: COMPLETE

Both phases must complete for payment.
```

### Points System

```
Request relay: 1 point
Response relay: 1 point
Exit (request): 1 point
Exit (response): 2 points (fetch work)

Total per round trip: ~7 points
Exit gets 3/7 ≈ 43%
```

---

## Budget

### Development

| Category | Cost |
|----------|------|
| Team (4 × 10 months) | $400K |
| Security audit | $50K |
| Infrastructure | $20K |
| Legal | $30K |
| Contingency | $50K |
| **Total** | **$550K** |

### Monthly Operations

| Category | Cost |
|----------|------|
| Bootstrap nodes | $5K |
| Monitoring | $1K |
| Support | $2K |
| **Total** | **$8K/month** |

---

## Success Metrics

### Technical

```
- [ ] <100ms routing latency (3 hops)
- [ ] 99.9% shard delivery
- [ ] Zero successful redirect attacks
```

### Adoption

```
- [ ] 10K+ users
- [ ] 1K+ node operators
- [ ] 100K+ daily requests
```

---

## Summary

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   CORE INNOVATION                                            │
│                                                              │
│   Relays verify: destination == origin                       │
│   Chain verifies: destination == stored user_pubkey          │
│                                                              │
│   Two layers of protection.                                  │
│   No trust required.                                         │
│   Attacks impossible.                                        │
│                                                              │
│   RESULT                                                     │
│                                                              │
│   Trustless VPN in 10 months.                                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

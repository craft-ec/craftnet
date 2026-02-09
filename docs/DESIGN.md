# TunnelCraft: P2P Incentivized VPN

## Executive Summary

TunnelCraft is a decentralized, trustless VPN network. It is **private but not anonymous** — no single node sees the full picture, and decentralization eliminates the single trust point that centralized VPNs require.

**Key Innovation**: Privacy through fragmentation + decentralized relay operators + trustless verification.

---

## Core Philosophy

```
No single trust point. Independent relay operators.
No trust required. Only cryptographic verification.
Best-effort routing. Network finds fastest path.
User controls privacy level (min relay count).
L4 TCP tunneling. TLS end-to-end.
```

---

## Architecture Overview

### Two Layers

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   LIBP2P                                                     │
│   • Kademlia DHT: peer/exit discovery                        │
│   • Gossipsub: subscription status, topology                 │
│   • NAT traversal via circuit relay                          │
│                                                              │
│   SOLANA                                                     │
│   • Subscriptions (tier + expiry)                            │
│   • Per-user reward pools                                    │
│   • ForwardReceipt submission + claims                       │
│   • Bandwidth-weighted settlement                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Components

| Layer | Technology | Purpose |
|-------|------------|---------|
| P2P | libp2p (Kademlia, gossipsub) | Discovery, NAT traversal, subscription gossip |
| Coding | Reed-Solomon (5/3, 3KB chunks) | Resilience, fragmentation |
| Routing | Best-effort with min relay count | Privacy, load distribution |
| Proof | ForwardReceipts (ed25519) | Proof of forwarding + bandwidth |
| Settlement | Solana (per-user pool) | Subscription + proportional claiming |

---

## Two Operating Modes

### TCP Tunnel Mode (L4, Primary)

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   Browser/App → SOCKS5 proxy (localhost:1080)                │
│     → TunnelCraft network → Exit node                        │
│       → Raw TCP connection to destination                    │
│                                                              │
│   Exit sees: host:port + TLS ciphertext                      │
│   Exit does NOT see: URLs, headers, request bodies           │
│   TLS is end-to-end: browser ↔ destination                   │
│                                                              │
│   Payload prefix: 0x01                                       │
│   Metadata: TunnelMetadata { host, port, session_id }        │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### HTTP Mode (L7, Legacy)

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   Client builds HTTP request shards                          │
│     → TunnelCraft network → Exit node                        │
│       → Exit fetches URL via reqwest                         │
│                                                              │
│   Exit sees: full HTTP request (URL, headers, body)          │
│   For HTTPS: exit terminates TLS with destination            │
│                                                              │
│   Payload prefix: 0x00 (default)                             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Routing Model

### Best-Effort with Minimum Relay Count

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   USER DECIDES                                               │
│   • Minimum relay count (0, 1, 2, 3)                         │
│   • Which exits to use                                       │
│                                                              │
│   NETWORK DECIDES                                            │
│   • Actual path (best-effort fastest)                        │
│   • Which relays                                             │
│                                                              │
│   NEVER DROPS                                                │
│   • hops_remaining > 0: forward to a relay, decrement        │
│   • hops_remaining = 0: forward toward exit                  │
│   • Can't reach exit directly: forward to a peer that can    │
│   • Shards are NEVER dropped due to missing peers            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Privacy Levels

| Mode | Min relays | Path | Privacy |
|------|-----------|------|---------|
| Direct | 0 | client → exit | Exit sees client IP |
| Light | 1 | client → relay → exit | 1 relay hides IP from exit |
| Standard | 2 | client → relay1 → relay2 → exit | No single node sees both |
| Paranoid | 3 | client → relay1 → relay2 → relay3 → exit | Maximum privacy |

---

## ForwardReceipts

### Proof of Forwarding

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   SHARD TRAVELS: User → A → B → Exit                         │
│                                                              │
│   User sends to A                                            │
│   A forwards to B → B signs receipt for A                    │
│   B forwards to Exit → Exit signs receipt for B              │
│                                                              │
│   Each receipt proves: "I received this shard"               │
│   Receipt includes: request_id, shard_id,                    │
│     sender_pubkey, receiver_pubkey, user_proof,              │
│     payload_size, epoch, timestamp, signature                │
│                                                              │
│   Receipts are the ONLY settlement primitive.                │
│   Bandwidth-weighted: payload_size matters.                  │
│   No credit indexes, no bitmap, no sequencer.                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Response Path Receipts

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   RESPONSE: Exit → X → Y → User                              │
│                                                              │
│   Exit sends to X → X signs receipt for Exit                 │
│   X sends to Y → Y signs receipt for X                       │
│   Y sends to User → User signs receipt for Y                 │
│                                                              │
│   Every node (including User) signs receipts                 │
│   Only the first relay on request path doesn't receive one   │
│   (User is the payer, not a claimable hop)                   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Trustless Verification

### The Key Insight

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   RELAYS VERIFY DESTINATION                                  │
│                                                              │
│   Request comes through:                                     │
│   Relay sees: {request_id, user_pubkey: ABC}                 │
│   Relay caches: request_id → user ABC                        │
│                                                              │
│   Response comes through:                                    │
│   Relay sees: {request_id, destination: XYZ}                 │
│   Relay checks: XYZ == ABC?                                  │
│   No → Drop. Won't forward.                                  │
│                                                              │
│   EXIT CAN'T REDIRECT                                        │
│   Relays enforce destination = origin                        │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Every Step Verified

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   USER                                                       │
│   Verified by: On-chain SubscriptionPDA (active tier)        │
│   Bound by: user_proof = SHA256(request_id || pubkey || sig) │
│                                                              │
│   RELAY                                                      │
│   Verified by: ForwardReceipt from next hop                  │
│   Validates: destination == origin (response path)           │
│   Settlement: Submits receipts to user's pool                │
│                                                              │
│   EXIT                                                       │
│   Verified by: ForwardReceipt from first response relay      │
│   Constrained by: Relays check destination                   │
│   Settlement: Submits receipts to user's pool                │
│                                                              │
│   CLIENT (on response)                                       │
│   Signs receipts so last relay can settle                    │
│                                                              │
│   TRUST REQUIRED: None                                       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Subscription + Per-User Pool Model

### Overview

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   1. USER SUBSCRIBES                                         │
│      On-chain: SubscriptionPDA { tier, expires_at }          │
│      Payment goes into UserPoolPDA { balance }               │
│                                                              │
│   2. RELAYS FORWARD SHARDS                                   │
│      Collect ForwardReceipts as proof of work                │
│      Receipts include payload_size for bandwidth weighting   │
│      user_proof binds receipts to user's pool                │
│      Subscribed users get priority processing                │
│      Non-subscribed users get best-effort                    │
│                                                              │
│   3. RELAYS SUBMIT RECEIPTS                                  │
│      submit_receipts(user_pool, receipts[])                  │
│      Deduped by (request_id, shard_index, receiver_pubkey)   │
│      Increments relay's receipt count for that pool          │
│                                                              │
│   4. END OF CYCLE: CLAIM REWARDS                             │
│      Weighted by bandwidth: sum(payload_size) per relay      │
│      relay_payout = relay_bandwidth / total_bandwidth * pool │
│      Pull-based: relay claims its weighted share             │
│                                                              │
│   5. POOL RESETS                                             │
│      Remaining balance carries over or refunds               │
│      Subscription renews or expires                          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Subscription Tiers

| Tier | Price/month | Bandwidth | Pool contribution |
|------|------------|-----------|-------------------|
| Basic | 5 USDC | 10 GB | 5 USDC |
| Standard | 15 USDC | 100 GB | 15 USDC |
| Premium | 40 USDC | 1 TB + best-effort beyond | 40 USDC |

### Why Per-User Pool (Not Global Pool)

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   GLOBAL POOL (BROKEN)                                       │
│   • Abuser + colluding exit spam traffic                     │
│   • Exit earns massive share of ENTIRE pool                  │
│   • Honest relays' earnings diluted                          │
│   • One bad actor breaks economics for everyone              │
│   → Pool inflation attack                                    │
│                                                              │
│   PER-USER POOL (CORRECT)                                    │
│   • Abuser spams traffic                                     │
│   • More receipts against THEIR OWN pool only                │
│   • Per-receipt value drops (40 USDC / 10000 receipts)       │
│   • Relays detect low yield → stop serving that user         │
│   • Honest users unaffected                                  │
│   → Abuse is self-correcting                                 │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Privacy Model

### Private But Not Anonymous

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   RELAYS SEE:                                                │
│   • user_pubkey (pseudonymous, not real identity)            │
│   • destination (exit pubkey)                                │
│   • opaque encrypted payload                                 │
│                                                              │
│   RELAYS DON'T SEE:                                          │
│   • Actual content (TLS end-to-end via SOCKS5)               │
│   • Client's real identity or IP (after 1+ relays)           │
│                                                              │
│   EXIT SEES:                                                 │
│   • host:port + TLS ciphertext (tunnel mode)                 │
│   • Cannot read content                                      │
│                                                              │
│   NO SINGLE ENTITY SEES THE FULL PICTURE                     │
│   Relay operators are independent, not one company           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Competitive Position

| Product | Hops | Centralized? | Content private? | Routing metadata hidden? | Trust model |
|---------|------|-------------|-----------------|-------------------------|-------------|
| NordVPN/ExpressVPN | 1 | Yes | Yes (HTTPS) | No — provider sees all | Trust the company |
| Mullvad multi-hop | 2 | Yes | Yes | Split — same company | Trust the company |
| **TunnelCraft** | **2+** | **No** | **Yes (TLS e2e)** | **Split — independent operators** | **No single trust point** |
| Tor | 3 | No | Yes | Yes — onion encryption | No trust needed |
| Nym | 3 | No | Yes | Yes + timing obfuscation | No trust needed |

---

## Subscription Verification

### Gossip-Based (Zero RPC)

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   1. User subscribes on-chain                                │
│   2. Listener node detects event                             │
│   3. Gossips subscription to network via gossipsub           │
│   4. All relays update local cache:                          │
│      cache[user_pubkey] = { tier, expires_at }               │
│                                                              │
│   RELAY RECEIVES SHARD:                                      │
│   • Check local cache for user_pubkey                        │
│   • Cache hit + not expired → priority queue                 │
│   • Cache miss → best-effort queue                           │
│                                                              │
│   RANDOM AUDIT:                                              │
│   • Relay spot-checks random users on-chain periodically     │
│   • Catches fake gossip messages                             │
│   • Fakers reported for abuse                                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Shard Structure

### Request Shard

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   {                                                          │
│     shard_id: bytes32,                                       │
│     request_id: bytes32,                                     │
│     user_pubkey: pubkey,                                     │
│     destination: exit_pubkey,                                │
│     user_proof: bytes32,        // SHA256(req_id||pk||sig)   │
│     hops_remaining: u8,         // Decremented per relay     │
│     total_hops: u8,             // Never decremented         │
│     sender_pubkey: pubkey,      // Last relay's identity     │
│     payload: encrypted,                                      │
│     shard_index: u8,            // 0-4 within chunk          │
│     total_shards: u8,           // 5                         │
│     chunk_index: u16,           // Which 3KB chunk           │
│     total_chunks: u16,          // Total chunks in request   │
│   }                                                          │
│                                                              │
│   No credit_hash, no credit_indexes, no ChainEntry vec.      │
│   sender_pubkey replaces the chain — only last hop tracked.  │
│   user_proof binds to settlement pool.                       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Erasure Coding

### Parameters

```
Data shards: 3
Parity shards: 2
Total shards: 5
Chunk size: 3KB (before encoding, ~1KB per shard)
Redundancy ratio: 1.67x
```

### Chunked Encoding

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   Large payload (e.g. 9KB)                                   │
│   → Split into 3KB chunks: [chunk_0, chunk_1, chunk_2]       │
│   → Each chunk → 5 shard payloads (~1KB each)                │
│   → Total: 3 chunks × 5 shards = 15 shards                  │
│                                                              │
│   Each shard carries chunk_index and total_chunks             │
│   Exit collects 3+ shards per chunk to reconstruct           │
│                                                              │
│   Small payload (<3KB) → 1 chunk → 5 shards                  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Attack Resistance

### All Attacks Blocked

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   ATTACK: Exit redirects response to colluding user         │
│   → Relay checks destination == cached origin → Drop         │
│                                                              │
│   ATTACK: Relay submits same receipt twice                   │
│   → Deduped by (request_id, shard_index, receiver_pubkey)    │
│                                                              │
│   ATTACK: Relay forges a ForwardReceipt                      │
│   → ed25519 signed by receiver — can't forge                 │
│                                                              │
│   ATTACK: Relay doesn't forward, claims receipt              │
│   → No forwarding → no receipt from next hop                 │
│                                                              │
│   ATTACK: User spams to inflate pool receipts                │
│   → Per-user pool: only dilutes own pool                     │
│   → Relays detect low yield → stop serving                   │
│                                                              │
│   ATTACK: Fake subscription via gossip                       │
│   → Random audit catches fakers in minutes                   │
│                                                              │
│   ATTACK: Receipt replay across epochs                       │
│   → ForwardReceipt.epoch prevents cross-epoch replay         │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Defense Layers

| Layer | Defender | Checks |
|-------|----------|--------|
| Routing | Relays | destination == origin |
| Subscription | Gossip + random audit | Active subscription verified |
| Proof of work | On-chain | ForwardReceipt signature valid |
| Anti-double-claim | On-chain | Receipt deduped by unique tuple |
| Anti-replay | On-chain | Epoch field prevents cross-epoch |
| Anti-abuse | Per-user pool | Abuse dilutes abuser's own pool only |
| Priority | Relays | Subscribed → priority, else best-effort |
| Settlement binding | user_proof | Receipts bound to specific user's pool |

---

## NAT Constraints

### Node Roles

| Role | Phone | Home (UPnP) | VPS |
|------|-------|-------------|-----|
| Client | Yes | Yes | Yes |
| Relay | No (pre-onion) | Yes (backbone) | Yes (backbone) |
| Exit | No | Yes | Yes |

**Phone cannot be exit**: TCP session pool, shard reconstruction, response encoding — too heavy on battery and mobile data.

**Phone cannot be relay (pre-onion)**: Only 10-30 connections, not enough for random peer selection. Future: with topology gossip + onion routing, phones CAN relay on explicit paths.

**Home users with UPnP are the relay backbone** — same role as seeders in BitTorrent.

### NAT Traversal: Circuit Relay

TunnelCraft uses circuit relay (libp2p) — works on ALL NAT types including symmetric NAT (mobile carriers). NATted nodes connect outbound to public relays, and all traffic flows through established connections.

---

## Mobile VPN Integration

```
App traffic
  → OS routes to TUN interface
    → VPN extension reads IP packets
      → tun2socks reassembles TCP, connects to SOCKS5
        → SOCKS5 proxy (Rust, localhost:1080)
          → TunnelCraft network
```

The SOCKS5 proxy runs inside the Rust library. The uniffi surface is just `start(port)` / `stop()` / `status()`. No uniffi bindings needed for SOCKS5 itself.

---

## Full Flow (Tunnel Mode)

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   1. USER SUBSCRIBES                                         │
│      • On-chain: tier + payment → pool PDA                   │
│      • Subscription event gossiped to network                │
│                                                              │
│   2. SOCKS5 CONNECT                                          │
│      • Browser sends CONNECT example.com:443                 │
│      • SOCKS5 proxy generates session_id                     │
│      • Buffers TCP bytes (50ms / 18KB flush)                 │
│                                                              │
│   3. SHARD CREATION                                          │
│      • Payload: [0x01][metadata_len][metadata][tcp_data]     │
│      • Chunk into 3KB pieces → 5 shards per chunk            │
│      • user_proof set on all shards                          │
│                                                              │
│   4. REQUEST ROUTING                                         │
│      • Shards sent to first relays                           │
│      • Each relay: check subscription, decrement hops,       │
│        forward, collect ForwardReceipt from next hop         │
│      • Relays cache: request_id → user_pubkey                │
│      • hops_remaining = 0: forward toward exit               │
│                                                              │
│   5. EXIT RECEIVES                                           │
│      • Reconstructs payload from 3+ shards per chunk         │
│      • Detects 0x01 prefix → tunnel mode                     │
│      • Parses TunnelMetadata → opens TCP to host:port        │
│      • Writes tcp_data to destination socket                 │
│      • Reads response bytes                                  │
│                                                              │
│   6. RESPONSE ROUTING                                        │
│      • Exit creates response shards from response bytes      │
│      • Relays check: destination == cached user_pubkey       │
│      • Each relay collects receipt from next hop              │
│                                                              │
│   7. DELIVERY                                                │
│      • Last relay delivers to user                           │
│      • User signs ForwardReceipt for last relay              │
│      • SOCKS5 proxy writes response to browser socket        │
│                                                              │
│   8. SETTLEMENT                                              │
│      • Relays submit receipts to user's pool on-chain        │
│      • Weighted by bandwidth (payload_size)                  │
│      • End of cycle: claim proportional share                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Summary

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   DISCOVERY:  libp2p Kademlia DHT                            │
│   ROUTING:    Best-effort with minimum relay count            │
│   TRANSPORT:  L4 TCP tunnel (SOCKS5) + L7 HTTP (legacy)      │
│   PRIVACY:    Private (not anonymous). TLS end-to-end.       │
│   PROOF:      ForwardReceipts (bandwidth-weighted)           │
│   SECURITY:   Relays verify destination = origin             │
│   BINDING:    user_proof = SHA256(req_id || pk || sig)        │
│   PAYMENT:    Subscription → per-user pool                   │
│   CLAIMING:   Proportional by bandwidth forwarded            │
│   GOSSIP:     Subscription status + random audit             │
│   TRUST:      None required                                  │
│                                                              │
│   Every step verified cryptographically.                     │
│   Abuse is self-correcting (per-user pool).                  │
│   No bitmap. No sequencer. No credit indexes.                │
│   ForwardReceipt is the only settlement primitive.           │
│   Market handles service quality.                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

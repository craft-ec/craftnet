# TunnelCraft Implementation Progress

## Summary

Tracking implementation progress across all major development milestones.

---

## Completed: Backend Architecture

### Core Types (crates/core)
- [x] Shard struct with `user_proof`, `sender_pubkey`, `chunk_index`, `total_chunks`
- [x] ForwardReceipt with `payload_size`, `epoch`, `shard_id`, `user_proof`
- [x] TunnelMetadata with `host`, `port`, `session_id`, `is_close`
- [x] HopMode enum with `min_relays()` (Direct/Light/Standard/Paranoid)
- [x] Payload mode constants (`PAYLOAD_MODE_HTTP = 0x00`, `PAYLOAD_MODE_TUNNEL = 0x01`)
- [x] ChainEntry replaced with fixed `sender_pubkey` field
- [x] SubscriptionTier enum (Basic/Standard/Premium)
- [x] ExitRegion enum with geographic options
- [x] Serialization/deserialization for all types

### Erasure Coding (crates/erasure)
- [x] Reed-Solomon 5/3 (3 data + 2 parity shards)
- [x] 3KB chunked encoding (`chunk_and_encode`)
- [x] Multi-chunk reassembly (`reassemble`)
- [x] 18KB max payload per burst (6 chunks)

### Cryptography (crates/crypto)
- [x] Ed25519 signing via SigningKeypair
- [x] X25519 key exchange
- [x] ChaCha20Poly1305 encryption
- [x] `sign_data` / `verify_data` functions

### Relay Logic (crates/relay)
- [x] Request handling with origin caching
- [x] Response handling with trustless destination verification
- [x] Best-effort routing (never drops shards)
- [x] `sender_pubkey` stamping (replaces chain signing)
- [x] Minimum relay count via `hops_remaining` decrement
- [x] Subscription check via gossip cache

### Exit Node (crates/exit)
- [x] Multi-chunk shard reconstruction
- [x] Dual-mode dispatch: tunnel (0x01) vs HTTP (0x00)
- [x] TCP tunnel handler with session pool
- [x] HTTP request execution via reqwest
- [x] Response shard creation (chunked)
- [x] Stale session cleanup

### Client SDK (crates/client)
- [x] TunnelCraftNode with event loop
- [x] RequestBuilder for HTTP mode
- [x] `build_tunnel_shards()` for tunnel mode
- [x] SOCKS5 proxy server (RFC 1928, CONNECT, NO AUTH)
- [x] Best-effort shard forwarding (fallback to any peer)
- [x] `user_proof` computation and binding
- [x] Pending request/tunnel tracking

### Settlement (crates/settlement)
- [x] Settlement client (mock mode)
- [x] Photon client for live mode
- [x] Remaining accounts builder
- [x] ForwardReceipt creation with bandwidth weighting

### Networking (crates/network)
- [x] libp2p swarm setup
- [x] Kademlia DHT for discovery
- [x] Gossipsub for subscription propagation
- [x] Circuit relay for NAT traversal
- [x] Wire protocol for shard transport

### Daemon (crates/daemon)
- [x] IPC server (JSON-RPC over Unix socket)
- [x] Windows named pipe support
- [x] Start/stop proxy commands
- [x] Daemon lifecycle management

---

## Completed: Frontend Apps

### Desktop (Electron) — 2025-01-27
- [x] Main process with daemon management
- [x] Preload script with context bridge
- [x] React UI with all components
- [x] System tray integration
- [x] electron-builder.yml configuration
- [x] macOS entitlements + notarize script
- [x] launchd plist for daemon auto-start
- [x] Windows NSIS installer script

### Desktop Additions — 2025-02-07
- [x] RequestPanel component (GET/POST, URL input, response display, history)
- [x] RequestPanel.css matching existing panel styling

### Mobile (React Native) — 2025-01-27
- [x] Shared UI components (StatusIndicator, ConnectButton, etc.)
- [x] VPN Context with dual-provide pattern
- [x] iOS Native Module (TunnelCraftVPNModule.swift)
- [x] iOS Network Extension (PacketTunnelProvider.swift)
- [x] Android Native Module (TunnelCraftVPNModule.kt)
- [x] Android VPN Service (TunnelCraftVpnService.kt)
- [x] NativeTunnelContext for production

### Mobile Additions — 2025-02-07
- [x] Unified TunnelContext (export + dual-provide)
- [x] Settings screen wiring (relay/exit toggles, purchase credits)
- [x] RequestScreen (method picker, URL/body input, response, history)
- [x] Native request bridge (Swift + Kotlin + TS)
- [x] App.tsx switched to `getRecommendedProvider()`

### iOS Configuration
- [x] TunnelCraft.entitlements (main app)
- [x] TunnelCraftVPN.entitlements (Network Extension)
- [x] TunnelCraftVPN/Info.plist (extension info)
- [x] App Group setup for Extension ↔ App communication

---

## Completed: Architecture Changes — 2025-02-08

### Best-Effort Routing
- [x] `HopMode.min_relays()` as primary method (replaces `hop_count()`)
- [x] Relay handler simplified — both request/response paths do same thing
- [x] `send_shards()` removed early "no relay peers" error
- [x] `forward_shard_inner()` updated with fallback paths (never drops shards)
- [x] Variable renames: `hops` → `min_relays` in request.rs and tunnel.rs

### Bandwidth-Weighted Settlement
- [x] ForwardReceipt includes `payload_size` field
- [x] Settlement weighted by bandwidth forwarded, not receipt count
- [x] 18KB chunk size for optimal shard payload

---

## Remaining Work

### High Priority
- [ ] Complete on-chain Solana settlement program
- [ ] End-to-end SOCKS5 tunnel testing through live network
- [ ] Integrate tun2socks for mobile system-wide VPN
- [ ] iOS Xcode project with Network Extension target
- [ ] Build UniFFI xcframework for iOS (arm64 + simulator)
- [ ] Build UniFFI .so for Android (arm64-v8a, armeabi-v7a)

### Medium Priority
- [ ] macOS icon.icns and DMG background
- [ ] Test notarization workflow
- [ ] Windows cross-compilation and testing
- [ ] Code signing for all platforms
- [ ] Auto-updater integration

### Future: Anonymity Layer
- [ ] Topology gossip (relay peer connectivity)
- [ ] Client topology graph + path selection
- [ ] Lease set publishing (gateway + tunnel_id)
- [ ] Onion encryption (Sphinx or simplified)
- [ ] Blind subscription tokens
- [ ] Participatory relay model for free users

---

## Testing Commands

```bash
# Rust backend
cargo test                              # All tests
cargo test -p tunnelcraft-core          # Core types
cargo test -p tunnelcraft-erasure       # Erasure coding
cargo test -p tunnelcraft-relay         # Relay handler
cargo test -p tunnelcraft-exit          # Exit handler
cargo test -p tunnelcraft-client        # Client SDK
cargo clippy                            # Lint

# Desktop
cd apps/desktop && npm run dev

# Mobile
cd apps/mobile && npx react-native run-ios
```

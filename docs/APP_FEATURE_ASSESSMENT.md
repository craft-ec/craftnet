# CraftNet App Feature Assessment

**Last Updated**: 2025-02-08

## Executive Summary

CraftNet is a decentralized P2P VPN operating at L4 (TCP tunnel via SOCKS5) and L7 (HTTP). The Rust backend is complete with SOCKS5 proxy, tunnel mode, best-effort routing, and bandwidth-weighted settlement. Frontend apps have full UI and native bridges. Main gaps are platform-specific packaging and tun2socks integration for mobile system-wide VPN.

---

## Current App State Assessment

### Backend (Rust Crates) - COMPLETE

| Crate | Status | Notes |
|-------|--------|-------|
| `core` | Complete | Shard, ForwardReceipt, TunnelMetadata, HopMode |
| `crypto` | Complete | Ed25519, X25519, ChaCha20 |
| `erasure` | Complete | Reed-Solomon 5/3, 3KB chunked encoding |
| `network` | Complete | libp2p, Kademlia, gossipsub, circuit relay |
| `relay` | Complete | Best-effort routing + trustless destination verification |
| `exit` | Complete | HTTP fetch + TCP tunnel handler (dual-mode) |
| `settlement` | Partial | Mock + Photon live modes, on-chain program pending |
| `client` | Complete | CraftNetNode + SOCKS5 proxy + tunnel builder |
| `daemon` | Complete | IPC server (Unix socket + Windows named pipe) |
| `uniffi` | Complete | iOS/Android FFI bindings |

### iOS App (React Native + Network Extension)

**Current State**: Structure complete, needs Xcode config + tun2socks

| Component | Status | Details |
|-----------|--------|---------|
| React Native UI | Complete | HomeScreen, SettingsScreen, RequestScreen |
| TunnelContext | Complete | Unified dual-provide pattern |
| Native Bridge (TS) | Complete | `CraftNetVPN.ts` with request method |
| CraftNetVPNModule.swift | Complete | RN native module |
| VPNManager.swift | Complete | NETunnelProviderManager wrapper |
| PacketTunnelProvider.swift | Complete | Full implementation |
| CraftNetCore.swift | Complete | UniFFI Swift wrapper |
| Entitlements | Complete | Main app + Network Extension |
| UniFFI xcframework | Needs Build | Framework structure exists |
| Xcode Network Extension | Needs Config | Target not fully configured |
| tun2socks integration | Not Started | For system-wide VPN |

### macOS App (Electron)

**Current State**: Good foundation, needs packaging

| Component | Status | Details |
|-----------|--------|---------|
| Electron main process | Complete | Window, tray, IPC handlers |
| React renderer | Complete | All UI components + RequestPanel |
| VPNContext | Complete | State management |
| IPC Client | Complete | Unix socket JSON-RPC |
| DaemonManager | Complete | Spawn/stop daemon |
| System tray | Complete | Menu integration |
| Window vibrancy | Complete | Native macOS feel |
| electron-builder config | Complete | electron-builder.yml created |
| launchd plist | Complete | Daemon auto-start |
| Notarize script | Complete | Apple notarization |
| DMG/PKG installer | Needs Testing | Config exists |
| Code signing | Not Configured | Apple Developer account needed |

### Windows App (Electron)

**Current State**: Shared Electron codebase, needs testing

| Component | Status | Details |
|-----------|--------|---------|
| Electron app | Complete (shared) | Same as macOS |
| Named Pipe IPC | Complete | `windows_pipe.rs` implemented |
| NSIS installer | Complete | `installer.nsh` created |
| Windows Service | Not Tested | Config in installer |
| Code signing | Not Configured | Certificate needed |

### Android App (React Native + VpnService)

**Current State**: Good structure, JNI bridge incomplete

| Component | Status | Details |
|-----------|--------|---------|
| React Native UI | Complete | Shared with iOS (includes RequestScreen) |
| CraftNetVpnService.kt | Complete | VpnService subclass |
| CraftNetVPNModule.kt | Complete | RN native module with request |
| JNI bindings | Partial | Methods declared, not linked |
| Gradle build | Partial | UniFFI not integrated |
| VPN permission request | Complete | In module |
| tun2socks integration | Not Started | For system-wide VPN |

---

## VPN Architecture

### Desktop
```
Apps → SOCKS5 proxy (localhost:1080) → CraftNet network
```
Apps configure SOCKS5 proxy manually or via system proxy settings.

### Mobile (System-Wide)
```
All app traffic
  → TUN interface (OS VPN tunnel)
    → tun2socks (IP packets → TCP streams)
      → SOCKS5 proxy (localhost:1080)
        → CraftNet network
```
The SOCKS5 proxy runs inside the Rust library. No separate uniffi bindings needed for it. The uniffi surface is just `start(port)` / `stop()` / `status()`.

### tun2socks Library Options

| Library | Language | Maturity | Notes |
|---------|----------|----------|-------|
| **tun2socks** (xjasonlyu) | Go | Production | Used by Outline, Orbot. Most battle-tested. |
| **tun2proxy** | Rust | Early | Fits Rust stack. Less proven. |
| **hev-socks5-tunnel** | C | Stable | Lightweight, easy to link. |

Decision deferred. Go tun2socks (proven) or Rust tun2proxy (stack consistency).

---

## Remaining Feature Gaps

### iOS
- [ ] Configure Xcode project with Network Extension target
- [ ] Build UniFFI xcframework for iOS (arm64 + simulator)
- [ ] Integrate tun2socks for system-wide VPN
- [ ] Set up provisioning profiles + code signing
- [ ] Test on physical device

### macOS
- [ ] Create icon.icns and DMG background
- [ ] Test notarization workflow
- [ ] Test launchd service
- [ ] Code signing setup

### Windows
- [ ] Cross-compile daemon for Windows
- [ ] Test named pipe IPC
- [ ] Test NSIS installer
- [ ] Code signing certificate

### Android
- [ ] Build UniFFI .so libraries (arm64-v8a, armeabi-v7a)
- [ ] Link JNI in Gradle
- [ ] Integrate tun2socks for system-wide VPN
- [ ] Test VpnService integration

### All Platforms
- [ ] End-to-end SOCKS5 tunnel testing through live network
- [ ] Auto-updater integration
- [ ] Crash reporting
- [ ] App Store / Play Store submissions

---

## Implementation Priority

### Phase 1: Integration Testing
1. End-to-end SOCKS5 tunnel through live CraftNet network
2. iOS Network Extension on physical device
3. macOS DMG build + test

### Phase 2: Mobile VPN
4. tun2socks integration (iOS + Android)
5. System-wide traffic capture testing
6. Battery/performance profiling

### Phase 3: Distribution
7. Code signing (Apple + Windows)
8. App Store / Play Store preparation
9. Auto-updater setup

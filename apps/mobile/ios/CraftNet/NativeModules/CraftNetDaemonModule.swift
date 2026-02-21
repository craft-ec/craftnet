import Foundation
import React
import NetworkExtension
import CryptoKit
import CraftNetCore

/// React Native Native Module for CraftNet Daemon
/// Provides full daemon functionality for VPN and node operations
/// Uses Rust FFI bindings via uniffi
@objc(CraftNetDaemon)
class CraftNetDaemonModule: RCTEventEmitter {

    private var hasListeners = false

    // Rust FFI objects
    private var unifiedNode: CraftNetUnifiedNode?
    private var settlementManager: SettlementManager?

    // Configuration state
    private var nodeMode: NodeMode = .both
    private var allowExit: Bool = true
    private var privacyLevel: PrivacyLevel = .direct
    private var bandwidthLimitMbps: Int = 0 // 0 = unlimited
    private var selectedExitId: String?

    // Session tracking
    private var sessionStartTime: Date?
    private var sessionBytesSent: UInt64 = 0
    private var sessionBytesReceived: UInt64 = 0

    // Helpers
    private let historyStorage = HistoryStorage.shared
    private let keychainManager = KeychainManager.shared
    private let vpnManager = VPNManager.shared

    // Timers
    private var statsTimer: Timer?
    private var epochTimer: Timer?

    // VPN state
    private var isVPNConnected = false
    private var vpnConfigured = false

    // Epoch duration (1 hour)
    private let epochDurationSecs: UInt64 = 3600

    // MARK: - React Native Setup

    override init() {
        super.init()
        initializeRustBindings()
        setupVPNManager()
    }

    private func setupVPNManager() {
        // Setup VPN status change handler
        vpnManager.onStatusChange = { [weak self] status in
            guard let self = self else { return }

            let stateString = self.vpnManager.statusString(status)
            print("[CraftNetDaemon] VPN status changed: \(stateString)")

            // Map NEVPNStatus to our connection states
            switch status {
            case .connected:
                self.isVPNConnected = true
                self.sendEventSafe(name: "onConnectionStateChange", body: "connected")
            case .connecting:
                self.sendEventSafe(name: "onConnectionStateChange", body: "connecting")
            case .disconnecting:
                self.sendEventSafe(name: "onConnectionStateChange", body: "disconnecting")
            case .disconnected:
                self.isVPNConnected = false
                self.sendEventSafe(name: "onConnectionStateChange", body: "disconnected")
            case .reasserting:
                self.sendEventSafe(name: "onConnectionStateChange", body: "connecting")
            case .invalid:
                self.isVPNConnected = false
                self.sendEventSafe(name: "onConnectionStateChange", body: "disconnected")
            @unknown default:
                break
            }
        }

        // Load VPN configuration on startup
        vpnManager.loadConfiguration { [weak self] error in
            if let error = error {
                print("[CraftNetDaemon] Failed to load VPN config: \(error)")
            } else {
                print("[CraftNetDaemon] VPN configuration loaded")
                self?.vpnConfigured = true
            }
        }
    }

    private func initializeRustBindings() {
        // Initialize the Rust library
        initLibrary()

        // Create settlement manager
        settlementManager = createSettlementManager()

        // Configure for devnet (uses built-in program ID)
        let config = createDevnetSettlementConfig()
        do {
            try settlementManager?.configure(config: config)
            print("[CraftNetDaemon] Settlement configured successfully")
        } catch {
            print("[CraftNetDaemon] Failed to configure settlement: \(error)")
        }

        // Load or generate keys and set on settlement manager
        do {
            let keypair = try loadOrGenerateKeys()
            let pubkeyHex = keypair.publicKey.map { String(format: "%02x", $0) }.joined()
            try settlementManager?.setNodePubkey(pubkeyHex: pubkeyHex)
            print("[CraftNetDaemon] Node pubkey set: \(pubkeyHex.prefix(16))...")

            // Also set credit hash (derived from pubkey for now)
            let creditHash = SHA256.hash(data: keypair.publicKey)
            let creditHashHex = creditHash.map { String(format: "%02x", $0) }.joined()
            try settlementManager?.setCreditHash(creditHashHex: creditHashHex)
            print("[CraftNetDaemon] Credit hash set: \(creditHashHex.prefix(16))...")

            print("[CraftNetDaemon] Rust bindings initialized successfully")
        } catch {
            print("[CraftNetDaemon] Failed to initialize keys: \(error)")
        }
    }

    /// Load existing keys from keychain or generate new ones
    private func loadOrGenerateKeys() throws -> (privateKey: Data, publicKey: Data) {
        if keychainManager.hasPrivateKey() {
            // Load existing key
            let privateKey = try keychainManager.loadPrivateKey()
            // Derive public key from private key (Ed25519: pubkey is last 32 bytes of 64-byte keypair, or derived)
            // For simplicity, if we stored 64-byte keypair, pubkey is last 32 bytes
            // If 32-byte seed, we need to derive it
            let publicKey: Data
            if privateKey.count == 64 {
                publicKey = privateKey.suffix(32)
            } else if privateKey.count == 32 {
                // Use CryptoKit to derive pubkey from seed
                let signingKey = try Curve25519.Signing.PrivateKey(rawRepresentation: privateKey)
                publicKey = signingKey.publicKey.rawRepresentation
            } else {
                throw KeychainManager.KeychainError.invalidKeyLength
            }
            print("[CraftNetDaemon] Loaded existing keys from keychain")
            return (privateKey, publicKey)
        } else {
            // Generate new keys using CryptoKit
            let signingKey = Curve25519.Signing.PrivateKey()
            let privateKey = signingKey.rawRepresentation
            let publicKey = signingKey.publicKey.rawRepresentation

            // Store in keychain
            try keychainManager.storePrivateKey(privateKey)
            print("[CraftNetDaemon] Generated and stored new keys")
            return (privateKey, publicKey)
        }
    }

    @objc override static func requiresMainQueueSetup() -> Bool {
        return true
    }

    override func supportedEvents() -> [String]! {
        return [
            "onConnectionStateChange",
            "onStatsUpdate",
            "onExitsUpdate",
            "onCreditsUpdate",
            "onEpochUpdate",
            "onPointsUpdate",
            "onError"
        ]
    }

    override func startObserving() {
        hasListeners = true
    }

    override func stopObserving() {
        hasListeners = false
    }

    // Required for NativeEventEmitter compatibility with New Architecture
    @objc override func addListener(_ eventName: String) {
        super.addListener(eventName)
    }

    @objc override func removeListeners(_ count: Double) {
        super.removeListeners(count)
    }

    // Safe event sending
    private func sendEventSafe(name: String, body: Any?) {
        guard hasListeners, let bridge = self.bridge, bridge.isValid else { return }
        sendEvent(withName: name, body: body)
    }

    // MARK: - Connection Methods

    @objc(connect:withResolver:withRejecter:)
    func connect(
        _ config: NSDictionary,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        print("[CraftNetDaemon] connect() called with config: \(config)")

        // Save configuration to shared UserDefaults for Network Extension
        if let defaults = UserDefaults(suiteName: "group.com.craftnet.vpn") {
            defaults.set(privacyLevelString(privacyLevel), forKey: "privacyLevel")
            defaults.set(bandwidthLimitMbps, forKey: "bandwidthLimitMbps")

            // Get credits from settlement manager
            if let settlement = settlementManager {
                do {
                    let balance = try settlement.getCreditBalance()
                    defaults.set(balance.balance, forKey: "credits")
                } catch {
                    defaults.set(1000, forKey: "credits") // Default
                }
            }

            defaults.synchronize()
        }

        // Ensure VPN is configured
        guard vpnConfigured else {
            print("[CraftNetDaemon] VPN not configured, loading configuration...")
            vpnManager.loadConfiguration { [weak self] error in
                if let error = error {
                    reject("VPN_CONFIG_ERROR", "Failed to configure VPN: \(error.localizedDescription)", error)
                    return
                }
                self?.vpnConfigured = true
                self?.startVPNTunnel(resolve: resolve, reject: reject)
            }
            return
        }

        startVPNTunnel(resolve: resolve, reject: reject)
    }

    private func startVPNTunnel(resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        // Check if running in fallback mode
        let inFallbackMode = vpnManager.isFallbackMode
        if inFallbackMode {
            print("[CraftNetDaemon] Running in fallback mode (P2P only, no system VPN)")
        } else {
            print("[CraftNetDaemon] Starting VPN tunnel...")
        }

        // Start session tracking
        sessionStartTime = Date()
        sessionBytesSent = 0
        sessionBytesReceived = 0

        // Start the VPN tunnel (or simulate in fallback mode)
        vpnManager.startTunnel { [weak self] error in
            guard let self = self else { return }

            if let error = error {
                print("[CraftNetDaemon] Failed to start VPN: \(error)")
                reject("VPN_START_ERROR", error.localizedDescription, error)
                return
            }

            if inFallbackMode {
                print("[CraftNetDaemon] Fallback mode - starting P2P node for discovery")
            } else {
                print("[CraftNetDaemon] VPN tunnel start requested successfully")
            }

            // Also start the P2P UnifiedNode for network discovery and connectivity
            self.startP2PNode()

            // Start stats updates
            DispatchQueue.main.async {
                self.startStatsUpdates()
                self.startEpochUpdates()
            }

            resolve(nil)
        }
    }

    /// Start the P2P UnifiedNode for network discovery
    private func startP2PNode() {
        // Don't start if already running
        guard unifiedNode == nil else {
            print("[CraftNetDaemon] P2P node already running")
            return
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            do {
                // Create node configuration
                let nodeConfig = createNodeConfig(
                    mode: self.nodeMode,
                    allowExit: self.allowExit,
                    privacyLevel: self.privacyLevel
                )

                // Create and start the node
                let node = try CraftNetUnifiedNode(config: nodeConfig)
                try node.start()

                self.unifiedNode = node
                print("[CraftNetDaemon] P2P node started successfully")

                // Set credits if available
                if let settlement = self.settlementManager {
                    do {
                        let balance = try settlement.getCreditBalance()
                        node.setCredits(credits: balance.balance)
                    } catch {
                        node.setCredits(credits: 1000) // Default credits
                    }
                }
            } catch {
                print("[CraftNetDaemon] Failed to start P2P node: \(error)")
                // Don't fail the connection - VPN/fallback mode still works
            }
        }
    }

    @objc(disconnect:withRejecter:)
    func disconnect(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        print("[CraftNetDaemon] disconnect() called")
        stopStatsUpdates()

        // Save connection record before disconnecting
        saveConnectionRecord(disconnectReason: "user_initiated")

        // Stop the VPN tunnel
        vpnManager.stopTunnel()

        // Also stop unified node if running (for P2P discovery)
        if let node = unifiedNode {
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                do {
                    try node.stop()
                    self?.unifiedNode = nil
                } catch {
                    print("[CraftNetDaemon] Error stopping unified node: \(error)")
                }
            }
        }

        // VPN status change will be handled by the observer
        resolve(nil)
    }

    // Legacy disconnect handling for unified node only
    private func disconnectLegacy(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        sendEventSafe(name: "onConnectionStateChange", body: "disconnecting")
        stopStatsUpdates()
        saveConnectionRecord(disconnectReason: "user_initiated")

        guard let node = unifiedNode else {
            sendEventSafe(name: "onConnectionStateChange", body: "disconnected")
            resolve(nil)
            return
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            do {
                try node.stop()
                self.unifiedNode = nil
                DispatchQueue.main.async {
                    self.sendEventSafe(name: "onConnectionStateChange", body: "disconnected")
                    resolve(nil)
                }
            } catch {
                DispatchQueue.main.async {
                    reject("DISCONNECT_ERROR", error.localizedDescription, error)
                }
            }
        }
    }

    private func saveConnectionRecord(disconnectReason: String?) {
        guard let startTime = sessionStartTime else { return }

        let endTime = Date()
        let duration = endTime.timeIntervalSince(startTime)

        let stats = unifiedNode?.getStats()

        let record = HistoryStorage.ConnectionRecord(
            id: UUID().uuidString,
            startTime: startTime,
            endTime: endTime,
            duration: duration,
            bytesSent: stats?.bytesSent ?? sessionBytesSent,
            bytesReceived: stats?.bytesReceived ?? sessionBytesReceived,
            privacyLevel: privacyLevelString(privacyLevel),
            exitNodeId: selectedExitId,
            disconnectReason: disconnectReason
        )

        historyStorage.saveConnectionRecord(record)
        sessionStartTime = nil
    }

    @objc(getConnectionState:withRejecter:)
    func getConnectionState(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        // Use VPN status as the primary connection state
        let vpnStatus = vpnManager.status
        resolve(vpnManager.statusString(vpnStatus))
    }

    // MARK: - Settlement Configuration

    @objc(configureSettlement:withResolver:withRejecter:)
    func configureSettlement(
        _ config: NSDictionary,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let mode = config["mode"] as? String else {
            reject("INVALID_CONFIG", "Missing mode field", nil)
            return
        }

        do {
            let settlementConfig: SettlementConfigFfi
            switch mode {
            case "devnet":
                settlementConfig = createDevnetSettlementConfig()
            case "mainnet":
                settlementConfig = createMainnetSettlementConfig()
            default:
                settlementConfig = createMockSettlementConfig()
            }

            try settlementManager?.configure(config: settlementConfig)
            resolve(nil)
        } catch {
            reject("SETTLEMENT_CONFIG_ERROR", error.localizedDescription, error)
        }
    }

    // MARK: - Exit Nodes

    @objc(getAvailableExits:withRejecter:)
    func getAvailableExits(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let node = unifiedNode, node.isConnected() else {
            resolve([])
            return
        }

        // Get peer count and construct available exits
        let peerCount = node.getPeerCount()
        var exits: [[String: Any]] = []

        // For now, represent connected peers as potential exits
        if peerCount > 0 {
            exits.append([
                "id": node.getPeerId(),
                "countryCode": "XX",
                "countryName": "P2P Network",
                "city": "Decentralized",
                "region": "global",
                "latencyMs": 50,
                "reputation": 95,
                "peerCount": peerCount
            ])
        }

        resolve(exits)
    }

    @objc(selectExit:withResolver:withRejecter:)
    func selectExit(
        _ exitId: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        selectedExitId = exitId
        resolve(nil)
    }

    @objc(getSelectedExit:withRejecter:)
    func getSelectedExit(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        if let exitId = selectedExitId {
            resolve(["id": exitId])
        } else {
            resolve(nil)
        }
    }

    // MARK: - Node Mode

    @objc(setNodeMode:allowExit:withResolver:withRejecter:)
    func setNodeMode(
        _ mode: String,
        allowExit: Bool,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        self.nodeMode = mode == "client" ? .client : mode == "node" ? .node : .both
        self.allowExit = allowExit

        if let node = unifiedNode {
            do {
                try node.setMode(mode: self.nodeMode)
            } catch {
                print("[CraftNetDaemon] Failed to set mode: \(error)")
            }
        }

        resolve(nil)
    }

    @objc(getNodeMode:withRejecter:)
    func getNodeMode(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        let modeStr: String
        if let node = unifiedNode {
            let mode = node.getMode()
            switch mode {
            case .client: modeStr = "client"
            case .node: modeStr = "node"
            case .both: modeStr = "both"
            }
        } else {
            switch nodeMode {
            case .client: modeStr = "client"
            case .node: modeStr = "node"
            case .both: modeStr = "both"
            }
        }

        resolve([
            "mode": modeStr,
            "allowExit": allowExit
        ])
    }

    // MARK: - Stats

    @objc(getStats:withRejecter:)
    func getStats(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let node = unifiedNode else {
            resolve([
                "bytesSent": 0,
                "bytesReceived": 0,
                "shardsRelayed": 0,
                "requestsExited": 0,
                "creditsEarned": 0,
                "creditsSpent": 0,
                "connectedPeers": 0,
                "uptimeSecs": 0
            ])
            return
        }

        let stats = node.getStats()
        resolve([
            "bytesSent": stats.bytesSent,
            "bytesReceived": stats.bytesReceived,
            "shardsRelayed": stats.shardsRelayed,
            "requestsExited": stats.requestsExited,
            "creditsEarned": stats.creditsEarned,
            "creditsSpent": stats.creditsSpent,
            "connectedPeers": stats.connectedPeers,
            "uptimeSecs": stats.uptimeSecs
        ])
    }

    @objc(getConnectionHistory:withRejecter:)
    func getConnectionHistory(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        let history = historyStorage.loadConnectionHistory()
        let result = history.map { record -> [String: Any] in
            return [
                "id": record.id,
                "startTime": Int(record.startTime.timeIntervalSince1970 * 1000),
                "endTime": record.endTime != nil ? Int(record.endTime!.timeIntervalSince1970 * 1000) : NSNull(),
                "duration": record.duration,
                "bytesSent": record.bytesSent,
                "bytesReceived": record.bytesReceived,
                "privacyLevel": record.privacyLevel,
                "exitNodeId": record.exitNodeId ?? NSNull(),
                "disconnectReason": record.disconnectReason ?? NSNull()
            ]
        }
        resolve(result)
    }

    @objc(getEarningsHistory:withRejecter:)
    func getEarningsHistory(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        let history = historyStorage.loadEarningsHistory()
        let result = history.map { record -> [String: Any] in
            return [
                "id": record.id,
                "timestamp": Int(record.timestamp.timeIntervalSince1970 * 1000),
                "epoch": record.epoch,
                "creditsEarned": record.creditsEarned,
                "shardsRelayed": record.shardsRelayed,
                "requestsExited": record.requestsExited,
                "rewardsClaimed": record.rewardsClaimed ?? NSNull(),
                "claimTxSignature": record.claimTxSignature ?? NSNull()
            ]
        }
        resolve(result)
    }

    // MARK: - Credits

    @objc(getCredits:withRejecter:)
    func getCredits(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        if let node = unifiedNode {
            resolve(node.getCredits())
        } else if let settlement = settlementManager {
            do {
                let balance = try settlement.getCreditBalance()
                resolve(balance.balance)
            } catch {
                resolve(0)
            }
        } else {
            resolve(0)
        }
    }

    @objc(purchaseCredits:paymentMethod:withResolver:withRejecter:)
    func purchaseCredits(
        _ amount: NSNumber,
        paymentMethod: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let settlement = settlementManager else {
            reject("NOT_INITIALIZED", "Settlement manager not initialized", nil)
            return
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            do {
                let result = try settlement.purchaseCredits(amount: amount.uint64Value)
                DispatchQueue.main.async {
                    if result.success {
                        // Update node credits if connected
                        if let node = self.unifiedNode {
                            let currentCredits = node.getCredits()
                            node.setCredits(credits: currentCredits + amount.uint64Value)
                        }
                        self.sendEventSafe(name: "onCreditsUpdate", body: amount.uint64Value)
                        resolve([
                            "success": true,
                            "txSignature": result.signature
                        ])
                    } else {
                        resolve([
                            "success": false,
                            "error": result.error ?? "Unknown error"
                        ])
                    }
                }
            } catch {
                DispatchQueue.main.async {
                    reject("PURCHASE_ERROR", error.localizedDescription, error)
                }
            }
        }
    }

    // MARK: - Speed Test

    @objc(runSpeedTest:withRejecter:)
    func runSpeedTest(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }

            let testId = "speed-\(Int(Date().timeIntervalSince1970 * 1000))"
            let timestamp = Date()

            // Get exit node info
            let exitNodeId = self.unifiedNode?.getPeerId() ?? "unknown"

            // Measure latency (ping)
            var latencies: [Int] = []
            for _ in 0..<5 {
                let start = Date()
                if let url = URL(string: "https://speed.cloudflare.com/__down?bytes=1") {
                    let semaphore = DispatchSemaphore(value: 0)
                    var success = false
                    URLSession.shared.dataTask(with: url) { _, _, error in
                        if error == nil { success = true }
                        semaphore.signal()
                    }.resume()
                    _ = semaphore.wait(timeout: .now() + 5)
                    if success {
                        let latency = Int(Date().timeIntervalSince(start) * 1000)
                        latencies.append(latency)
                    }
                }
            }

            let avgLatency = latencies.isEmpty ? 50 : latencies.reduce(0, +) / latencies.count
            let jitter = latencies.count > 1 ?
                Double(latencies.max()! - latencies.min()!) / Double(latencies.count) : 2.0
            let packetLoss = Double(5 - latencies.count) / 5.0 * 100

            // Measure download speed (10MB)
            var downloadMbps = 50.0
            if let url = URL(string: "https://speed.cloudflare.com/__down?bytes=10000000") {
                let start = Date()
                let semaphore = DispatchSemaphore(value: 0)
                var receivedBytes = 0
                URLSession.shared.dataTask(with: url) { data, _, _ in
                    receivedBytes = data?.count ?? 0
                    semaphore.signal()
                }.resume()
                _ = semaphore.wait(timeout: .now() + 30)
                let elapsed = Date().timeIntervalSince(start)
                if elapsed > 0 && receivedBytes > 0 {
                    downloadMbps = Double(receivedBytes * 8) / elapsed / 1_000_000
                }
            }

            // Measure upload speed (2MB)
            var uploadMbps = 20.0
            if let url = URL(string: "https://speed.cloudflare.com/__up") {
                var request = URLRequest(url: url)
                request.httpMethod = "POST"
                let testData = Data(repeating: 0, count: 2_000_000)
                let start = Date()
                let semaphore = DispatchSemaphore(value: 0)
                var success = false
                URLSession.shared.uploadTask(with: request, from: testData) { _, _, error in
                    success = error == nil
                    semaphore.signal()
                }.resume()
                _ = semaphore.wait(timeout: .now() + 30)
                let elapsed = Date().timeIntervalSince(start)
                if elapsed > 0 && success {
                    uploadMbps = Double(testData.count * 8) / elapsed / 1_000_000
                }
            }

            // Save speed test record
            let record = HistoryStorage.SpeedTestRecord(
                id: testId,
                timestamp: timestamp,
                downloadMbps: downloadMbps,
                uploadMbps: uploadMbps,
                latencyMs: avgLatency,
                jitterMs: jitter,
                packetLoss: packetLoss,
                exitNodeId: exitNodeId,
                exitCountry: "XX"
            )
            self.historyStorage.saveSpeedTestRecord(record)

            DispatchQueue.main.async {
                resolve([
                    "id": testId,
                    "timestamp": Int(timestamp.timeIntervalSince1970 * 1000),
                    "downloadMbps": downloadMbps,
                    "uploadMbps": uploadMbps,
                    "latencyMs": avgLatency,
                    "jitterMs": jitter,
                    "packetLoss": packetLoss,
                    "exitNodeId": exitNodeId,
                    "exitCountry": "XX"
                ])
            }
        }
    }

    @objc(getSpeedTestHistory:withRejecter:)
    func getSpeedTestHistory(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        let history = historyStorage.loadSpeedTestHistory()
        let result = history.map { record -> [String: Any] in
            return [
                "id": record.id,
                "timestamp": Int(record.timestamp.timeIntervalSince1970 * 1000),
                "downloadMbps": record.downloadMbps,
                "uploadMbps": record.uploadMbps,
                "latencyMs": record.latencyMs,
                "jitterMs": record.jitterMs,
                "packetLoss": record.packetLoss,
                "exitNodeId": record.exitNodeId,
                "exitCountry": record.exitCountry
            ]
        }
        resolve(result)
    }

    // MARK: - Keys

    @objc(getPublicKey:withRejecter:)
    func getPublicKey(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        if let node = unifiedNode {
            resolve(node.getPeerId())
        } else {
            reject("NOT_CONNECTED", "Node not connected", nil)
        }
    }

    @objc(getNodeId:withRejecter:)
    func getNodeId(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        if let node = unifiedNode {
            resolve(node.getPeerId())
        } else {
            reject("NOT_CONNECTED", "Node not connected", nil)
        }
    }

    @objc(getCreditHash:withRejecter:)
    func getCreditHash(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        if let settlement = settlementManager {
            do {
                let balance = try settlement.getCreditBalance()
                resolve(balance.creditHash)
            } catch {
                reject("SETTLEMENT_ERROR", error.localizedDescription, error)
            }
        } else {
            reject("NOT_INITIALIZED", "Settlement manager not initialized", nil)
        }
    }

    @objc(exportPrivateKey:withResolver:withRejecter:)
    func exportPrivateKey(
        _ password: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard !password.isEmpty else {
            reject("INVALID_PASSWORD", "Password cannot be empty", nil)
            return
        }

        guard password.count >= 8 else {
            reject("WEAK_PASSWORD", "Password must be at least 8 characters", nil)
            return
        }

        do {
            let encryptedKey = try keychainManager.exportPrivateKey(password: password)
            resolve(encryptedKey)
        } catch {
            reject("EXPORT_FAILED", error.localizedDescription, error)
        }
    }

    @objc(importPrivateKey:password:withResolver:withRejecter:)
    func importPrivateKey(
        _ encryptedKey: String,
        password: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard !encryptedKey.isEmpty else {
            reject("INVALID_KEY", "Encrypted key cannot be empty", nil)
            return
        }

        guard !password.isEmpty else {
            reject("INVALID_PASSWORD", "Password cannot be empty", nil)
            return
        }

        do {
            try keychainManager.importPrivateKey(encryptedKey: encryptedKey, password: password)
            resolve(true)
        } catch {
            reject("IMPORT_FAILED", error.localizedDescription, error)
        }
    }

    // MARK: - Settings

    @objc(setPrivacyLevel:withResolver:withRejecter:)
    func setPrivacyLevel(
        _ level: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        privacyLevel = level == "direct" ? .direct :
                       level == "light" ? .light :
                       level == "paranoid" ? .paranoid : .standard

        if let node = unifiedNode {
            node.setPrivacyLevel(level: privacyLevel)
        }

        // Store in shared defaults for Network Extension
        UserDefaults(suiteName: "group.com.craftnet.vpn")?.set(level, forKey: "privacyLevel")

        resolve(nil)
    }

    @objc(setBandwidthLimit:withResolver:withRejecter:)
    func setBandwidthLimit(
        _ mbps: NSNumber,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        bandwidthLimitMbps = mbps.intValue

        // Store in shared defaults
        UserDefaults(suiteName: "group.com.craftnet.vpn")?.set(mbps.intValue, forKey: "bandwidthLimitMbps")

        // Apply limit if node is running
        // Note: Bandwidth limiting is enforced at the packet handling level
        // by controlling the rate of tunnelPacket calls

        resolve(nil)
    }

    @objc(setExitEnabled:withResolver:withRejecter:)
    func setExitEnabled(
        _ enabled: Bool,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        allowExit = enabled
        resolve(nil)
    }

    // MARK: - Split Tunneling

    @objc(setSplitTunnelRules:mode:withResolver:withRejecter:)
    func setSplitTunnelRules(
        _ rules: NSArray,
        mode: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        // Convert NSArray to [[String: Any]]
        var rulesArray: [[String: Any]] = []
        for item in rules {
            if let rule = item as? [String: Any] {
                rulesArray.append(rule)
            }
        }

        print("[CraftNetDaemon] Setting split tunnel rules: \(rulesArray.count) rules, mode: \(mode)")

        // Use VPNManager to update rules
        vpnManager.updateSplitTunnelRules(rulesArray, mode: mode)

        resolve(nil)
    }

    @objc(getSplitTunnelRules:withRejecter:)
    func getSplitTunnelRules(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let defaults = UserDefaults(suiteName: "group.com.craftnet.vpn") else {
            resolve([
                "mode": "exclude",
                "rules": []
            ])
            return
        }

        let mode = defaults.string(forKey: "splitTunnelMode") ?? "exclude"
        let rules = defaults.array(forKey: "splitTunnelRules") ?? []

        resolve([
            "mode": mode,
            "rules": rules
        ])
    }

    // MARK: - Epoch & Settlement

    @objc(getEpochInfo:withRejecter:)
    func getEpochInfo(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let settlement = settlementManager else {
            reject("NOT_INITIALIZED", "Settlement manager not initialized", nil)
            return
        }

        do {
            let info = try settlement.getEpochInfo()
            resolve([
                "currentEpoch": info.currentEpoch,
                "epochStartTime": info.epochStartTime,
                "epochEndTime": info.epochEndTime,
                "epochDurationSecs": info.epochDurationSecs,
                "timeRemainingMs": info.timeRemainingMs
            ])
        } catch {
            reject("EPOCH_ERROR", error.localizedDescription, error)
        }
    }

    @objc(getNodePoints:withRejecter:)
    func getNodePoints(
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let settlement = settlementManager else {
            reject("NOT_INITIALIZED", "Settlement manager not initialized", nil)
            return
        }

        do {
            let points = try settlement.getNodePoints()
            resolve([
                "pendingShards": points.pendingShards,
                "pendingPoints": points.pendingPoints,
                "lifetimePoints": points.lifetimePoints,
                "availableRewards": points.availableRewardsUsdc
            ])
        } catch {
            reject("POINTS_ERROR", error.localizedDescription, error)
        }
    }

    @objc(getClaimHistory:withResolver:withRejecter:)
    func getClaimHistory(
        _ limit: NSNumber,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        // Get claim history from local storage
        var history = historyStorage.loadEpochHistory()
            .filter { $0.claimed }
            .map { record -> [String: Any] in
                return [
                    "id": "claim-\(record.epoch)",
                    "timestamp": record.claimTimestamp != nil ? Int(record.claimTimestamp!.timeIntervalSince1970 * 1000) : Int(record.endTime.timeIntervalSince1970 * 1000),
                    "shardsSettled": record.pointsEarned, // Using points as proxy for shards
                    "pointsEarned": record.pointsEarned,
                    "txSignature": record.claimTxSignature ?? NSNull()
                ]
            }

        // Limit results
        let maxResults = limit.intValue > 0 ? limit.intValue : 10
        if history.count > maxResults {
            history = Array(history.prefix(maxResults))
        }

        resolve(history)
    }

    @objc(getEpochHistory:withResolver:withRejecter:)
    func getEpochHistory(
        _ limit: NSNumber,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        var history = historyStorage.loadEpochHistory()

        // Limit results
        let maxResults = limit.intValue > 0 ? limit.intValue : 10
        if history.count > maxResults {
            history = Array(history.prefix(maxResults))
        }

        let result = history.map { record -> [String: Any] in
            return [
                "epoch": record.epoch,
                "startTime": Int(record.startTime.timeIntervalSince1970 * 1000),
                "endTime": Int(record.endTime.timeIntervalSince1970 * 1000),
                "pointsEarned": record.pointsEarned,
                "claimed": record.claimed,
                "claimTxSignature": record.claimTxSignature ?? NSNull(),
                "claimTimestamp": record.claimTimestamp != nil ? Int(record.claimTimestamp!.timeIntervalSince1970 * 1000) : NSNull()
            ]
        }
        resolve(result)
    }

    @objc(claimRewards:withResolver:withRejecter:)
    func claimRewards(
        _ epoch: NSNumber,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let settlement = settlementManager else {
            reject("NOT_INITIALIZED", "Settlement manager not initialized", nil)
            return
        }

        let epochInt = epoch.intValue

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            do {
                let result = try settlement.withdrawRewards(credits: 0) // 0 = claim all for epoch
                DispatchQueue.main.async {
                    if result.success {
                        // Mark epoch as claimed in local history
                        self.historyStorage.markEpochClaimed(epoch: epochInt, txSignature: result.signature)

                        self.sendPointsUpdate()
                        resolve([
                            "success": true,
                            "txSignature": result.signature,
                            "amount": 0
                        ])
                    } else {
                        resolve([
                            "success": false,
                            "amount": 0,
                            "error": result.error ?? "Unknown error"
                        ])
                    }
                }
            } catch {
                DispatchQueue.main.async {
                    reject("CLAIM_ERROR", error.localizedDescription, error)
                }
            }
        }
    }

    @objc(withdrawRewards:withResolver:withRejecter:)
    func withdrawRewards(
        _ amount: NSNumber,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let settlement = settlementManager else {
            reject("NOT_INITIALIZED", "Settlement manager not initialized", nil)
            return
        }

        let withdrawAmount = amount.uint64Value

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            do {
                let result = try settlement.withdrawRewards(credits: withdrawAmount)
                DispatchQueue.main.async {
                    if result.success {
                        self.sendPointsUpdate()
                        resolve([
                            "success": true,
                            "txSignature": result.signature,
                            "amount": withdrawAmount
                        ])
                    } else {
                        resolve([
                            "success": false,
                            "amount": 0,
                            "error": result.error ?? "Unknown error"
                        ])
                    }
                }
            } catch {
                DispatchQueue.main.async {
                    reject("WITHDRAW_ERROR", error.localizedDescription, error)
                }
            }
        }
    }

    // MARK: - Timers

    private func startStatsUpdates() {
        statsTimer?.invalidate()
        statsTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            self?.updateStats()
        }
    }

    private func stopStatsUpdates() {
        statsTimer?.invalidate()
        statsTimer = nil
        epochTimer?.invalidate()
        epochTimer = nil
    }

    private func startEpochUpdates() {
        epochTimer?.invalidate()
        epochTimer = Timer.scheduledTimer(withTimeInterval: 10.0, repeats: true) { [weak self] _ in
            self?.sendEpochUpdate()
            self?.recordEpochEarnings()
        }
    }

    private func updateStats() {
        guard let node = unifiedNode, node.isConnected() else { return }

        let stats = node.getStats()

        // Track session stats
        sessionBytesSent = stats.bytesSent
        sessionBytesReceived = stats.bytesReceived

        sendEventSafe(name: "onStatsUpdate", body: [
            "bytesSent": stats.bytesSent,
            "bytesReceived": stats.bytesReceived,
            "shardsRelayed": stats.shardsRelayed,
            "requestsExited": stats.requestsExited,
            "creditsEarned": stats.creditsEarned,
            "creditsSpent": stats.creditsSpent,
            "connectedPeers": stats.connectedPeers,
            "uptimeSecs": stats.uptimeSecs
        ])
    }

    private func sendEpochUpdate() {
        guard hasListeners, let bridge = self.bridge, bridge.isValid else { return }

        guard let settlement = settlementManager else { return }

        do {
            let info = try settlement.getEpochInfo()
            sendEvent(withName: "onEpochUpdate", body: [
                "currentEpoch": info.currentEpoch,
                "epochStartTime": info.epochStartTime,
                "epochEndTime": info.epochEndTime,
                "epochDurationSecs": info.epochDurationSecs,
                "timeRemainingMs": info.timeRemainingMs
            ])
        } catch {
            print("[CraftNetDaemon] Failed to get epoch info: \(error)")
        }
    }

    private func sendPointsUpdate() {
        guard hasListeners, let bridge = self.bridge, bridge.isValid else { return }
        guard let settlement = settlementManager else { return }

        do {
            let points = try settlement.getNodePoints()
            sendEvent(withName: "onPointsUpdate", body: [
                "pendingShards": points.pendingShards,
                "pendingPoints": points.pendingPoints,
                "lifetimePoints": points.lifetimePoints,
                "availableRewards": points.availableRewardsUsdc
            ])
        } catch {
            print("[CraftNetDaemon] Failed to get node points: \(error)")
        }
    }

    private func recordEpochEarnings() {
        guard let node = unifiedNode, node.isConnected() else { return }
        guard let settlement = settlementManager else { return }

        do {
            let epochInfo = try settlement.getEpochInfo()
            let stats = node.getStats()
            let currentEpoch = Int(epochInfo.currentEpoch)

            // Save epoch record
            let epochRecord = HistoryStorage.EpochRecord(
                epoch: currentEpoch,
                startTime: Date(timeIntervalSince1970: Double(epochInfo.epochStartTime) / 1000),
                endTime: Date(timeIntervalSince1970: Double(epochInfo.epochEndTime) / 1000),
                pointsEarned: stats.creditsEarned,
                claimed: false,
                claimTxSignature: nil,
                claimTimestamp: nil
            )
            historyStorage.saveEpochRecord(epochRecord)

            // Save earnings record periodically
            let earningsRecord = HistoryStorage.EarningsRecord(
                id: UUID().uuidString,
                timestamp: Date(),
                epoch: currentEpoch,
                creditsEarned: stats.creditsEarned,
                shardsRelayed: stats.shardsRelayed,
                requestsExited: stats.requestsExited,
                rewardsClaimed: nil,
                claimTxSignature: nil
            )
            historyStorage.saveEarningsRecord(earningsRecord)
        } catch {
            print("[CraftNetDaemon] Failed to record epoch earnings: \(error)")
        }
    }

    // MARK: - Helpers

    private func privacyLevelString(_ level: PrivacyLevel) -> String {
        switch level {
        case .direct: return "direct"
        case .light: return "light"
        case .standard: return "standard"
        case .paranoid: return "paranoid"
        }
    }
}

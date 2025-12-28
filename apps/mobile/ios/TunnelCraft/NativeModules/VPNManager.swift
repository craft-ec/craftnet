import Foundation
import NetworkExtension
import os.log

/// Manages the VPN tunnel lifecycle using NETunnelProviderManager
/// This bridges the main app to the PacketTunnelProvider Network Extension
class VPNManager {
    static let shared = VPNManager()

    private let log = OSLog(subsystem: "com.tunnelcraft.app", category: "VPNManager")
    private var manager: NETunnelProviderManager?
    private var statusObserver: NSObjectProtocol?

    // Callbacks
    var onStatusChange: ((NEVPNStatus) -> Void)?

    // App Group for sharing data with Network Extension
    private let appGroup = "group.com.tunnelcraft.vpn"

    // Check if running on simulator
    private var isSimulator: Bool {
        #if targetEnvironment(simulator)
        return true
        #else
        return false
        #endif
    }

    // Fallback mode when VPN capabilities are not available
    private(set) var isFallbackMode = false

    private init() {
        setupStatusObserver()
    }

    deinit {
        if let observer = statusObserver {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    // MARK: - Status Observer

    private func setupStatusObserver() {
        statusObserver = NotificationCenter.default.addObserver(
            forName: .NEVPNStatusDidChange,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let connection = notification.object as? NEVPNConnection else { return }
            self?.handleStatusChange(connection.status)
        }
    }

    private func handleStatusChange(_ status: NEVPNStatus) {
        os_log("VPN status changed: %{public}@", log: log, type: .info, statusString(status))
        onStatusChange?(status)
    }

    // MARK: - Configuration

    /// Load or create VPN configuration
    func loadConfiguration(completion: @escaping (Error?) -> Void) {
        // VPN APIs don't work on simulator - use fallback mode
        if isSimulator {
            os_log("VPN not available on simulator - using fallback mode", log: log, type: .info)
            isFallbackMode = true
            completion(nil)
            return
        }

        NETunnelProviderManager.loadAllFromPreferences { [weak self] managers, error in
            guard let self = self else { return }

            if let error = error {
                os_log("Failed to load VPN configurations: %{public}@", log: self.log, type: .error, error.localizedDescription)

                // Check if it's a permission error - enable fallback mode
                let nsError = error as NSError
                if nsError.domain == "NEVPNErrorDomain" || nsError.localizedDescription.contains("permission denied") {
                    os_log("VPN permission denied - enabling fallback mode (P2P only, no system VPN)", log: self.log, type: .info)
                    os_log("To enable full VPN: enroll in Apple Developer Program ($99/year) and enable Network Extension capability", log: self.log, type: .info)
                    self.isFallbackMode = true
                    // Don't return error - allow app to continue in fallback mode
                    completion(nil)
                    return
                }

                completion(error)
                return
            }

            if let existingManager = managers?.first {
                os_log("Found existing VPN configuration", log: self.log, type: .info)
                self.manager = existingManager
                self.isFallbackMode = false
                completion(nil)
            } else {
                os_log("No existing VPN configuration, creating new one", log: self.log, type: .info)
                self.createConfiguration(completion: completion)
            }
        }
    }

    private func createConfiguration(completion: @escaping (Error?) -> Void) {
        let manager = NETunnelProviderManager()

        // Protocol configuration
        let proto = NETunnelProviderProtocol()
        proto.providerBundleIdentifier = "com.tunnelcraft.TunnelCraft.TunnelCraftVPN"
        proto.serverAddress = "TunnelCraft P2P Network"
        proto.disconnectOnSleep = false

        manager.protocolConfiguration = proto
        manager.localizedDescription = "TunnelCraft VPN"
        manager.isEnabled = true

        // Save configuration
        manager.saveToPreferences { [weak self] error in
            guard let self = self else { return }

            if let error = error {
                os_log("Failed to save VPN configuration: %{public}@", log: self.log, type: .error, error.localizedDescription)
                completion(error)
                return
            }

            os_log("VPN configuration saved", log: self.log, type: .info)

            // Reload to get the saved configuration
            manager.loadFromPreferences { error in
                if let error = error {
                    os_log("Failed to reload VPN configuration: %{public}@", log: self.log, type: .error, error.localizedDescription)
                    completion(error)
                    return
                }

                self.manager = manager
                completion(nil)
            }
        }
    }

    // MARK: - Tunnel Control

    // Simulator mock state
    private var simulatorMockConnected = false

    /// Start the VPN tunnel
    func startTunnel(options: [String: Any]? = nil, completion: @escaping (Error?) -> Void) {
        // In fallback mode (simulator or no VPN capability), simulate connection
        if isFallbackMode {
            os_log("Starting in fallback mode (P2P only, no system VPN routing)", log: log, type: .info)
            simulatorMockConnected = true
            // Simulate async connection
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.onStatusChange?(.connected)
            }
            completion(nil)
            return
        }

        guard let manager = manager else {
            let error = NSError(domain: "VPNManager", code: 1, userInfo: [NSLocalizedDescriptionKey: "VPN not configured"])
            completion(error)
            return
        }

        os_log("Starting VPN tunnel", log: log, type: .info)

        do {
            // Convert options to NSObject dictionary for the tunnel
            var tunnelOptions: [String: NSObject]?
            if let opts = options {
                tunnelOptions = opts.mapValues { value -> NSObject in
                    if let str = value as? String { return str as NSObject }
                    if let num = value as? Int { return num as NSObject }
                    if let bool = value as? Bool { return bool as NSObject }
                    return "\(value)" as NSObject
                }
            }

            try manager.connection.startVPNTunnel(options: tunnelOptions)
            completion(nil)
        } catch {
            os_log("Failed to start VPN tunnel: %{public}@", log: log, type: .error, error.localizedDescription)
            completion(error)
        }
    }

    /// Stop the VPN tunnel
    func stopTunnel() {
        if isFallbackMode {
            os_log("Stopping fallback mode connection", log: log, type: .info)
            simulatorMockConnected = false
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
                self?.onStatusChange?(.disconnected)
            }
            return
        }

        os_log("Stopping VPN tunnel", log: log, type: .info)
        manager?.connection.stopVPNTunnel()
    }

    /// Get current VPN status
    var status: NEVPNStatus {
        if isFallbackMode {
            return simulatorMockConnected ? .connected : .disconnected
        }
        return manager?.connection.status ?? .invalid
    }

    /// Check if VPN is connected
    var isConnected: Bool {
        return status == .connected
    }

    // MARK: - Split Tunneling

    /// Update split tunnel rules in shared UserDefaults
    func updateSplitTunnelRules(_ rules: [[String: Any]], mode: String) {
        guard let defaults = UserDefaults(suiteName: appGroup) else {
            os_log("Failed to access app group defaults", log: log, type: .error)
            return
        }

        defaults.set(rules, forKey: "splitTunnelRules")
        defaults.set(mode, forKey: "splitTunnelMode") // "include" or "exclude"
        defaults.synchronize()

        os_log("Updated split tunnel rules: %d rules, mode: %{public}@", log: log, type: .info, rules.count, mode)

        // If connected, notify the extension to reload rules
        if isConnected {
            sendMessageToExtension("reloadSplitTunnelRules")
        }
    }

    /// Update privacy level
    func updatePrivacyLevel(_ level: String) {
        guard let defaults = UserDefaults(suiteName: appGroup) else { return }
        defaults.set(level, forKey: "privacyLevel")
        defaults.synchronize()

        if isConnected {
            sendMessageToExtension("setPrivacyLevel:\(level)")
        }
    }

    /// Update bandwidth limit
    func updateBandwidthLimit(_ mbps: Int) {
        guard let defaults = UserDefaults(suiteName: appGroup) else { return }
        defaults.set(mbps, forKey: "bandwidthLimitMbps")
        defaults.synchronize()

        if isConnected {
            sendMessageToExtension("setBandwidthLimit:\(mbps)")
        }
    }

    /// Update credits
    func updateCredits(_ credits: UInt64) {
        guard let defaults = UserDefaults(suiteName: appGroup) else { return }
        defaults.set(credits, forKey: "credits")
        defaults.synchronize()

        if isConnected {
            sendMessageToExtension("setCredits:\(credits)")
        }
    }

    // MARK: - Extension Communication

    /// Send a message to the Network Extension
    func sendMessageToExtension(_ message: String, completion: ((Data?) -> Void)? = nil) {
        guard let session = manager?.connection as? NETunnelProviderSession else {
            os_log("No active tunnel session", log: log, type: .error)
            completion?(nil)
            return
        }

        guard let data = message.data(using: .utf8) else {
            completion?(nil)
            return
        }

        do {
            try session.sendProviderMessage(data) { response in
                completion?(response)
            }
        } catch {
            os_log("Failed to send message to extension: %{public}@", log: log, type: .error, error.localizedDescription)
            completion?(nil)
        }
    }

    /// Get stats from the Network Extension
    func getStats(completion: @escaping ([String: Any]?) -> Void) {
        sendMessageToExtension("getStats") { data in
            guard let data = data,
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                completion(nil)
                return
            }
            completion(json)
        }
    }

    // MARK: - Helpers

    func statusString(_ status: NEVPNStatus) -> String {
        switch status {
        case .invalid: return "invalid"
        case .disconnected: return "disconnected"
        case .connecting: return "connecting"
        case .connected: return "connected"
        case .reasserting: return "reconnecting"
        case .disconnecting: return "disconnecting"
        @unknown default: return "unknown"
        }
    }
}

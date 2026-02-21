package com.craftnet.vpn

import android.content.Intent
import android.net.VpnService
import android.os.ParcelFileDescriptor
import android.util.Log
import java.io.FileInputStream
import java.io.FileOutputStream
import java.util.concurrent.atomic.AtomicBoolean

/**
 * CraftNet VPN Service
 *
 * Handles the VPN tunnel on Android by capturing and routing packets
 * through the CraftNet P2P network.
 */
class CraftNetVpnService : VpnService() {

    companion object {
        private const val TAG = "CraftNetVPN"

        // VPN configuration
        private const val VPN_ADDRESS = "10.8.0.2"
        private const val VPN_ROUTE = "0.0.0.0"
        private const val VPN_DNS = "1.1.1.1"
        private const val VPN_MTU = 1400

        // Actions
        const val ACTION_CONNECT = "com.craftnet.vpn.CONNECT"
        const val ACTION_DISCONNECT = "com.craftnet.vpn.DISCONNECT"

        // Extras
        const val EXTRA_PRIVACY_LEVEL = "privacy_level"
        const val EXTRA_BOOTSTRAP_PEER = "bootstrap_peer"
    }

    private var vpnInterface: ParcelFileDescriptor? = null
    private var isRunning = AtomicBoolean(false)
    private var packetThread: Thread? = null

    // Stats
    private var bytesSent: Long = 0
    private var bytesReceived: Long = 0

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        return when (intent?.action) {
            ACTION_CONNECT -> {
                val privacyLevel = intent.getStringExtra(EXTRA_PRIVACY_LEVEL) ?: "standard"
                val bootstrapPeer = intent.getStringExtra(EXTRA_BOOTSTRAP_PEER)
                connect(privacyLevel, bootstrapPeer)
                START_STICKY
            }
            ACTION_DISCONNECT -> {
                disconnect()
                START_NOT_STICKY
            }
            else -> START_NOT_STICKY
        }
    }

    private fun connect(privacyLevel: String, bootstrapPeer: String?) {
        if (isRunning.get()) {
            Log.w(TAG, "VPN already running")
            return
        }

        Log.i(TAG, "Starting VPN with privacy level: $privacyLevel")

        try {
            // Initialize Rust library
            initializeRustLibrary()

            // Create VPN interface
            vpnInterface = createVpnInterface()

            if (vpnInterface == null) {
                Log.e(TAG, "Failed to create VPN interface")
                return
            }

            isRunning.set(true)

            // Start packet handling thread
            packetThread = Thread {
                handlePackets()
            }.apply {
                name = "CraftNet-Packets"
                start()
            }

            Log.i(TAG, "VPN connected")

        } catch (e: Exception) {
            Log.e(TAG, "Failed to start VPN", e)
            disconnect()
        }
    }

    private fun disconnect() {
        Log.i(TAG, "Disconnecting VPN")

        isRunning.set(false)

        packetThread?.interrupt()
        packetThread = null

        vpnInterface?.close()
        vpnInterface = null

        stopSelf()

        Log.i(TAG, "VPN disconnected")
    }

    private fun createVpnInterface(): ParcelFileDescriptor? {
        return Builder()
            .setSession("CraftNet")
            .addAddress(VPN_ADDRESS, 24)
            .addRoute(VPN_ROUTE, 0)
            .addDnsServer(VPN_DNS)
            .setMtu(VPN_MTU)
            .setBlocking(true)
            .establish()
    }

    private fun handlePackets() {
        val vpnFd = vpnInterface?.fileDescriptor ?: return
        val input = FileInputStream(vpnFd)
        val output = FileOutputStream(vpnFd)

        val packet = ByteArray(VPN_MTU)

        try {
            while (isRunning.get()) {
                // Read packet from VPN interface
                val length = input.read(packet)

                if (length > 0) {
                    bytesSent += length

                    // Process packet through Rust tunnel
                    val responsePacket = tunnelPacket(packet.copyOf(length))

                    if (responsePacket != null && responsePacket.isNotEmpty()) {
                        bytesReceived += responsePacket.size
                        output.write(responsePacket)
                    }
                }
            }
        } catch (e: InterruptedException) {
            Log.d(TAG, "Packet thread interrupted")
        } catch (e: Exception) {
            Log.e(TAG, "Error in packet handling", e)
        } finally {
            input.close()
            output.close()
        }
    }

    private fun initializeRustLibrary() {
        // Load the Rust library
        try {
            System.loadLibrary("craftnet_uniffi")
            // Call Rust init function via JNI
            nativeInitLibrary()
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "Failed to load Rust library", e)
            throw e
        }
    }

    private fun tunnelPacket(packet: ByteArray): ByteArray? {
        // Call Rust tunnel function via JNI
        return try {
            nativeTunnelPacket(packet)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to tunnel packet", e)
            null
        }
    }

    override fun onDestroy() {
        disconnect()
        super.onDestroy()
    }

    override fun onRevoke() {
        disconnect()
        super.onRevoke()
    }

    // JNI methods - implemented in Rust via UniFFI
    private external fun nativeInitLibrary()
    private external fun nativeTunnelPacket(packet: ByteArray): ByteArray?
}

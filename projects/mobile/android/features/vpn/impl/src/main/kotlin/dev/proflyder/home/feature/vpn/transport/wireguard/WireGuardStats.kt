package dev.proflyder.home.feature.vpn.transport.wireguard

import com.wireguard.android.backend.GoBackend
import dev.proflyder.home.feature.vpn.VpnTraffic

internal data class WireGuardStats(val rxBytes: Long = 0, val txBytes: Long = 0, val latestHandshakeMillis: Long = 0) {
    val traffic: VpnTraffic get() = VpnTraffic(rxBytes, txBytes)
}

internal object WireGuardUapi {
    fun parse(text: String?): WireGuardStats {
        val pairs = text.orEmpty().lineSequence().mapNotNull(::pair).toList()
        val seconds = sum(pairs, "last_handshake_time_sec")
        val nanos = sum(pairs, "last_handshake_time_nsec")
        return WireGuardStats(
            rxBytes = sum(pairs, "rx_bytes"),
            txBytes = sum(pairs, "tx_bytes"),
            latestHandshakeMillis = seconds * MILLIS_IN_SECOND + nanos / NANOS_IN_MILLI,
        )
    }

    private fun pair(line: String): Pair<String, Long>? {
        val key = line.substringBefore('=', "")
        val value = line.substringAfter('=', "").toLongOrNull() ?: return null
        return key to value
    }

    private fun sum(pairs: List<Pair<String, Long>>, key: String): Long =
        pairs.filter { it.first == key }.sumOf { it.second }

    private const val MILLIS_IN_SECOND = 1_000L
    private const val NANOS_IN_MILLI = 1_000_000L
}

internal fun interface WireGuardStatsSource {
    fun read(): WireGuardStats
}

internal class GoBackendUapi(private val backend: () -> GoBackend) : WireGuardStatsSource {
    override fun read(): WireGuardStats = WireGuardUapi.parse(uapi())

    private fun uapi(): String? = runCatching { configReader.invoke(null, handleField.getInt(backend())) as? String }
        .getOrNull()

    private val handleField by lazy { GoBackend::class.java.getDeclaredField(HANDLE_FIELD).apply { isAccessible = true } }

    private val configReader by lazy {
        GoBackend::class.java.getDeclaredMethod(CONFIG_METHOD, Int::class.javaPrimitiveType).apply { isAccessible = true }
    }

    companion object {
        const val HANDLE_FIELD = "currentTunnelHandle"
        const val CONFIG_METHOD = "wgGetConfig"
    }
}

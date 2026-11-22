package dev.proflyder.home.feature.vpn

import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.VpnOcservSettings
import dev.proflyder.home.core.model.VpnWireGuardSettings
import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.net.URI

object VpnRoutes {
    fun allowedIps(settings: VpnWireGuardSettings, route: VpnRoute): List<String> = when {
        route == VpnRoute.HOME && settings.splitAllowedIps.isNotEmpty() -> settings.splitAllowedIps
        else -> settings.allowedIps
    }

    fun ocservGroup(settings: VpnOcservSettings, route: VpnRoute): String = when {
        route == VpnRoute.HOME && settings.splitGroup.isNotBlank() -> settings.splitGroup
        else -> settings.group
    }
}

object VpnEndpoints {
    fun select(settings: VpnWireGuardSettings, atHome: Boolean?): String = when {
        atHome == true && settings.lanEndpoint.isNotBlank() -> settings.lanEndpoint
        else -> settings.endpoint
    }
}

object WireGuardConfigText {
    fun render(
        settings: VpnWireGuardSettings,
        privateKey: String,
        route: VpnRoute,
        atHome: Boolean? = false,
    ): String = buildString {
        appendLine("[Interface]")
        appendLine("PrivateKey = $privateKey")
        appendLine("Address = ${settings.address}")
        appendList("DNS", settings.dns)
        appendPositive("MTU", settings.mtu)
        appendLine()
        appendLine("[Peer]")
        appendLine("PublicKey = ${settings.serverPublicKey}")
        appendLine("Endpoint = ${VpnEndpoints.select(settings, atHome)}")
        appendList("AllowedIPs", VpnRoutes.allowedIps(settings, route))
        appendPositive("PersistentKeepalive", settings.persistentKeepalive)
    }

    private fun StringBuilder.appendList(name: String, values: List<String>) {
        if (values.isNotEmpty()) appendLine("$name = ${values.joinToString(", ")}")
    }

    private fun StringBuilder.appendPositive(name: String, value: Int) {
        if (value > 0) appendLine("$name = $value")
    }
}

enum class HandshakeVerdict {
    WAIT,
    CONNECTED,
    FALLBACK,
}

object HandshakePolicy {
    const val TIMEOUT_MILLIS: Long = 10_000

    const val EXTERNAL_TIMEOUT_MILLIS: Long = 180_000

    fun timeoutFor(protocol: VpnProtocol): Long = when (protocol) {
        VpnProtocol.WIREGUARD -> TIMEOUT_MILLIS
        VpnProtocol.OPENCONNECT -> EXTERNAL_TIMEOUT_MILLIS
    }

    fun verdict(
        startedAtMillis: Long,
        nowMillis: Long,
        latestHandshakeMillis: Long,
        timeoutMillis: Long = TIMEOUT_MILLIS,
    ): HandshakeVerdict = when {
        latestHandshakeMillis > 0 -> HandshakeVerdict.CONNECTED
        nowMillis - startedAtMillis >= timeoutMillis -> HandshakeVerdict.FALLBACK
        else -> HandshakeVerdict.WAIT
    }

    fun afterWireGuardTimeout(atHome: Boolean?): VpnConnection = when (atHome) {
        true -> VpnConnection.NotNeededAtHome
        else -> VpnConnection.FallbackRequired
    }
}

object HomeNetworkRules {
    fun isHomeAddress(address: InetAddress): Boolean = when (address) {
        is Inet4Address -> address.isSiteLocalAddress
        is Inet6Address -> isUniqueLocal(address) || address.isSiteLocalAddress
        else -> false
    }

    fun shouldConnect(autoConnect: Boolean, atHome: Boolean, active: Boolean): Boolean =
        autoConnect && !atHome && !active

    fun shouldDisconnect(autoConnect: Boolean, atHome: Boolean, active: Boolean): Boolean =
        autoConnect && atHome && active

    fun shouldSwitchEndpoint(
        autoConnect: Boolean,
        atHome: Boolean,
        wireGuardActive: Boolean,
        tunnelEndpoint: String,
        wantedEndpoint: String,
    ): Boolean = wireGuardActive && tunnelEndpoint != wantedEndpoint &&
        !shouldDisconnect(autoConnect, atHome, wireGuardActive)

    private fun isUniqueLocal(address: Inet6Address): Boolean =
        address.address[0].toInt() and ULA_MASK == ULA_PREFIX

    private const val ULA_MASK = 0xFE
    private const val ULA_PREFIX = 0xFC
}

object VpnProblems {
    fun of(failure: RemoteFailure): VpnProfileProblem = when (failure) {
        RemoteFailure.NotConfigured -> VpnProfileProblem.NOT_CONFIGURED
        RemoteFailure.Unauthorized -> VpnProfileProblem.UNAUTHORIZED
        is RemoteFailure.Offline, RemoteFailure.Timeout, is RemoteFailure.Tls -> VpnProfileProblem.OFFLINE
        is RemoteFailure.Server -> ofCode(failure.code)
        else -> VpnProfileProblem.SERVER
    }

    private fun ofCode(code: String): VpnProfileProblem = when (code) {
        "http_400" -> VpnProfileProblem.INVALID
        "http_409" -> VpnProfileProblem.CONFLICT
        "http_429" -> VpnProfileProblem.LIMIT
        "http_503" -> VpnProfileProblem.UNAVAILABLE
        else -> VpnProfileProblem.SERVER
    }
}

object OpenConnectLink {
    const val SCHEME = "anyconnect"

    fun host(settings: VpnOcservSettings): String =
        runCatching { URI(settings.url).host }.getOrNull().orEmpty()

    fun connect(settings: VpnOcservSettings, route: VpnRoute, name: String): String? {
        val host = host(settings).ifBlank { return null }
        val group = VpnRoutes.ocservGroup(settings, route)
        val target = when (group.isBlank()) {
            true -> host
            false -> "$host/$group"
        }
        return "$SCHEME://connect/?name=${encode(name)}&host=${encode(target)}"
    }

    private fun encode(value: String): String = java.net.URLEncoder.encode(value, Charsets.UTF_8.name())
}

data class WireGuardStats(val rxBytes: Long = 0, val txBytes: Long = 0, val latestHandshakeMillis: Long = 0)

object WireGuardUapi {
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

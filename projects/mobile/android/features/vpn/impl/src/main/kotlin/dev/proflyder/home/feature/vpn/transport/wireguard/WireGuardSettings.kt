package dev.proflyder.home.feature.vpn.transport.wireguard

import dev.proflyder.home.core.model.LocalJson
import dev.proflyder.home.core.model.TransportSettings
import dev.proflyder.home.feature.vpn.VpnRoute
import kotlinx.serialization.Serializable

@Serializable
internal data class WireGuardInterface(val address: String, val dns: List<String> = emptyList(), val mtu: Int = 0)

@Serializable
internal data class WireGuardPeer(
    val publicKey: String,
    val endpoint: String,
    val lanEndpoint: String = "",
    val allowedIps: List<String> = emptyList(),
    val persistentKeepalive: Int = 0,
)

@Serializable
internal data class WireGuardSettings(
    val `interface`: WireGuardInterface,
    val peer: WireGuardPeer,
    val splitAllowedIps: List<String> = emptyList(),
) {
    fun endpoint(atHome: Boolean?): String = when {
        atHome == true && peer.lanEndpoint.isNotBlank() -> peer.lanEndpoint
        else -> peer.endpoint
    }

    fun allowedIps(route: VpnRoute): List<String> = when {
        route == VpnRoute.HOME && splitAllowedIps.isNotEmpty() -> splitAllowedIps
        else -> peer.allowedIps
    }

    companion object {
        fun of(settings: TransportSettings): WireGuardSettings? =
            runCatching { LocalJson.decodeFromJsonElement(serializer(), settings.json) }.getOrNull()
    }
}

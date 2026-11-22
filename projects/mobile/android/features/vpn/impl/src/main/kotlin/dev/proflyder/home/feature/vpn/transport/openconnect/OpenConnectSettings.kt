package dev.proflyder.home.feature.vpn.transport.openconnect

import dev.proflyder.home.core.model.LocalJson
import dev.proflyder.home.core.model.TransportSettings
import dev.proflyder.home.feature.vpn.VpnRoute
import java.net.URI
import java.net.URLEncoder
import kotlinx.serialization.Serializable

@Serializable
internal data class OpenConnectSettings(
    val url: String = "",
    val group: String = "",
    val splitGroup: String = "",
    val authentication: List<String> = emptyList(),
) {
    fun group(route: VpnRoute): String = when {
        route == VpnRoute.HOME && splitGroup.isNotBlank() -> splitGroup
        else -> group
    }

    fun host(): String = runCatching { URI(url).host }.getOrNull().orEmpty()

    companion object {
        fun of(settings: TransportSettings): OpenConnectSettings? =
            runCatching { LocalJson.decodeFromJsonElement(serializer(), settings.json) }.getOrNull()
    }
}

internal object OpenConnectLink {
    const val SCHEME = "anyconnect"
    const val DISCONNECT = "$SCHEME://disconnect"

    fun connect(settings: OpenConnectSettings, route: VpnRoute, name: String): String? {
        val host = settings.host().ifBlank { return null }
        val group = settings.group(route)
        val target = when (group.isBlank()) {
            true -> host
            false -> "$host/$group"
        }
        return "$SCHEME://connect/?name=${encode(name)}&host=${encode(target)}"
    }

    private fun encode(value: String): String = URLEncoder.encode(value, Charsets.UTF_8.name())
}

package dev.proflyder.home.feature.vpn.transport.wireguard

import dev.proflyder.home.feature.vpn.VpnRoute

internal object WireGuardConfigText {
    fun render(settings: WireGuardSettings, privateKey: String, route: VpnRoute, atHome: Boolean?): String = buildString {
        appendLine("[Interface]")
        appendLine("PrivateKey = $privateKey")
        appendLine("Address = ${settings.`interface`.address}")
        appendList("DNS", settings.`interface`.dns)
        appendPositive("MTU", settings.`interface`.mtu)
        appendLine()
        appendLine("[Peer]")
        appendLine("PublicKey = ${settings.peer.publicKey}")
        appendLine("Endpoint = ${settings.endpoint(atHome)}")
        appendList("AllowedIPs", settings.allowedIps(route))
        appendPositive("PersistentKeepalive", settings.peer.persistentKeepalive)
    }

    private fun StringBuilder.appendList(name: String, values: List<String>) {
        if (values.isNotEmpty()) appendLine("$name = ${values.joinToString(", ")}")
    }

    private fun StringBuilder.appendPositive(name: String, value: Int) {
        if (value > 0) appendLine("$name = $value")
    }
}

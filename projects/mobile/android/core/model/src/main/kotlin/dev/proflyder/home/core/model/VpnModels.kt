package dev.proflyder.home.core.model

data class VpnWireGuardSettings(
    val address: String,
    val dns: List<String>,
    val mtu: Int,
    val serverPublicKey: String,
    val endpoint: String,
    val allowedIps: List<String>,
    val splitAllowedIps: List<String>,
    val persistentKeepalive: Int,
    val lanEndpoint: String = "",
)

data class VpnOcservSettings(
    val url: String,
    val group: String,
    val splitGroup: String,
    val authentication: List<String>,
)

data class VpnProfile(
    val deviceId: String,
    val deviceName: String,
    val wireguard: VpnWireGuardSettings,
    val ocserv: VpnOcservSettings,
    val createdUnix: Long,
    val updatedUnix: Long,
)

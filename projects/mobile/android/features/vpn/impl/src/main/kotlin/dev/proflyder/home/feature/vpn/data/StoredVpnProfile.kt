package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.core.model.VpnOcservSettings
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnWireGuardSettings
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
internal data class StoredVpnProfile(
    val deviceId: String,
    val deviceName: String,
    val address: String,
    val dns: List<String>,
    val mtu: Int,
    val serverPublicKey: String,
    val endpoint: String,
    val allowedIps: List<String>,
    val splitAllowedIps: List<String>,
    val persistentKeepalive: Int,
    val ocservUrl: String,
    val ocservGroup: String,
    val ocservSplitGroup: String,
    val ocservAuthentication: List<String>,
    val createdUnix: Long,
    val updatedUnix: Long,
) {
    fun toModel(): VpnProfile = VpnProfile(
        deviceId = deviceId,
        deviceName = deviceName,
        wireguard = VpnWireGuardSettings(
            address, dns, mtu, serverPublicKey, endpoint, allowedIps, splitAllowedIps, persistentKeepalive,
        ),
        ocserv = VpnOcservSettings(ocservUrl, ocservGroup, ocservSplitGroup, ocservAuthentication),
        createdUnix = createdUnix,
        updatedUnix = updatedUnix,
    )

    companion object {
        private val json = Json { ignoreUnknownKeys = true }

        fun of(profile: VpnProfile): StoredVpnProfile = with(profile.wireguard) {
            StoredVpnProfile(
                profile.deviceId, profile.deviceName, address, dns, mtu, serverPublicKey, endpoint,
                allowedIps, splitAllowedIps, persistentKeepalive, profile.ocserv.url, profile.ocserv.group,
                profile.ocserv.splitGroup, profile.ocserv.authentication, profile.createdUnix, profile.updatedUnix,
            )
        }

        fun encode(profile: VpnProfile): String = json.encodeToString(serializer(), of(profile))

        fun decode(text: String?): VpnProfile? = text
            ?.let { runCatching { json.decodeFromString(serializer(), it) }.getOrNull() }
            ?.toModel()
    }
}

package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.core.model.LocalJson
import dev.proflyder.home.core.model.TransportSettings
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnProtocolId
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

@Serializable
internal data class StoredVpnProfile(
    val deviceId: String,
    val deviceName: String,
    val transports: Map<String, JsonObject>,
    val createdUnix: Long,
    val updatedUnix: Long,
) {
    fun toModel(): VpnProfile = VpnProfile(
        deviceId = deviceId,
        deviceName = deviceName,
        transports = transports.entries.associate { (id, json) -> VpnProtocolId(id) to TransportSettings(json) },
        createdUnix = createdUnix,
        updatedUnix = updatedUnix,
    )

    companion object {
        fun of(profile: VpnProfile): StoredVpnProfile = StoredVpnProfile(
            deviceId = profile.deviceId,
            deviceName = profile.deviceName,
            transports = profile.transports.entries.associate { (id, settings) -> id.value to settings.json },
            createdUnix = profile.createdUnix,
            updatedUnix = profile.updatedUnix,
        )

        fun encode(profile: VpnProfile): String = LocalJson.encodeToString(serializer(), of(profile))

        fun decode(text: String?): VpnProfile? = text
            ?.let { runCatching { LocalJson.decodeFromString(serializer(), it) }.getOrNull() }
            ?.toModel()
    }
}

package dev.proflyder.home.core.model

import kotlinx.serialization.json.JsonObject

@JvmInline
value class VpnProtocolId(val value: String)

@JvmInline
value class TransportSettings(val json: JsonObject)

data class VpnProfile(
    val deviceId: String,
    val deviceName: String,
    val transports: Map<VpnProtocolId, TransportSettings>,
    val createdUnix: Long,
    val updatedUnix: Long,
) {
    fun settingsOf(protocol: VpnProtocolId): TransportSettings? = transports[protocol]
}

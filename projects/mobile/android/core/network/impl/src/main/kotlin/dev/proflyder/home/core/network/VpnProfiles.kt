package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.proflyder.home.core.model.TransportSettings
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnProtocolId
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject

@Serializable
internal data class VpnProfileBody(val deviceId: String, val publicKey: String, val deviceName: String)

@Serializable
internal data class VpnRemoveBody(val deviceId: String)

@Serializable
internal data class VpnProfileResponseDto(val profile: JsonObject)

@Serializable
internal data class VpnRemovedResponseDto(val removed: Boolean = false)

internal interface VpnApi {
    @retrofit2.http.POST("v1/vpn/profile")
    suspend fun register(@retrofit2.http.Body body: VpnProfileBody): Outcome<VpnProfileResponseDto>

    @retrofit2.http.POST("v1/vpn/profile/remove")
    suspend fun remove(@retrofit2.http.Body body: VpnRemoveBody): Outcome<VpnRemovedResponseDto>
}

internal fun JsonObject.toVpnProfile(): VpnProfile = VpnProfile(
    deviceId = text("deviceId"),
    deviceName = text("deviceName"),
    transports = entries
        .filter { (key, value) -> key !in PROFILE_FIELDS && value is JsonObject }
        .associate { (key, value) -> VpnProtocolId(key) to TransportSettings(value.jsonObject) },
    createdUnix = text("createdUnix").toLongOrNull() ?: 0,
    updatedUnix = text("updatedUnix").toLongOrNull() ?: 0,
)

private fun JsonObject.text(key: String): String = (get(key) as? JsonPrimitive)?.contentOrNull.orEmpty()

private val PROFILE_FIELDS = setOf("deviceId", "deviceName", "createdUnix", "updatedUnix")

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpVpnProfileClient(private val http: HomeHttp) : VpnProfileApi {
    private val api: VpnApi by lazy { http.rest(VpnApi::class, Target.Scheduler) }

    override suspend fun register(
        deviceId: String,
        publicKey: String,
        deviceName: String,
    ): Outcome<VpnProfile> =
        api.register(
            VpnProfileBody(
                deviceId = deviceId.take(MAX_DEVICE_ID_CHARS),
                publicKey = publicKey,
                deviceName = deviceName.take(MAX_NAME_CHARS),
            ),
        ).mapResult { it.profile.toVpnProfile() }

    override suspend fun remove(deviceId: String): Outcome<Boolean> =
        api.remove(VpnRemoveBody(deviceId.take(MAX_DEVICE_ID_CHARS))).mapResult { it.removed }

    private companion object {
        const val MAX_NAME_CHARS = 64

        const val MAX_DEVICE_ID_CHARS = 128
    }
}

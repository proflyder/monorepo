package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.serialization.Serializable

@Serializable
internal data class RegisterDeviceBody(
    val deviceId: String,
    val token: String,
    val platform: String = "android",
    val appVersion: String = "",
    val deviceName: String = "",
)

@Serializable
internal data class RemoveDeviceBody(val token: String)

@Serializable
internal data class RenameDeviceBody(val deviceId: String, val deviceName: String)

@Serializable
internal data class PushDeviceResponseDto(val device: PushDeviceDto? = null)

@Serializable
internal data class PushDeviceDto(
    val deviceId: String = "",
    val token: String = "",
    val platform: String = "",
    val appVersion: String = "",
    val deviceName: String = "",
    val createdUnix: String = "0",
    val updatedUnix: String = "0",
)

@Serializable
internal data class RemovedDeviceResponseDto(val removed: Boolean = false)

internal interface PushApi {
    @retrofit2.http.POST("v1/push/devices")
    suspend fun register(
        @retrofit2.http.Body body: RegisterDeviceBody,
    ): Outcome<PushDeviceResponseDto>

    @retrofit2.http.POST("v1/push/devices/remove")
    suspend fun remove(
        @retrofit2.http.Body body: RemoveDeviceBody,
    ): Outcome<RemovedDeviceResponseDto>

    @retrofit2.http.POST("v1/push/devices/rename")
    suspend fun rename(
        @retrofit2.http.Body body: RenameDeviceBody,
    ): Outcome<PushDeviceResponseDto>
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpPushDevicesClient(private val http: HomeHttp) : PushDevicesApi {
    private val api: PushApi by lazy { http.rest(PushApi::class, Target.Scheduler) }

    override suspend fun register(
        deviceId: String,
        token: String,
        appVersion: String,
        deviceName: String,
    ): Outcome<Unit> =
        api.register(
            RegisterDeviceBody(
                deviceId = deviceId.take(MAX_DEVICE_ID_CHARS),
                token = token,
                appVersion = appVersion.take(MAX_META_CHARS),
                deviceName = deviceName.take(MAX_META_CHARS),
            ),
        ).mapResult { }

    override suspend fun rename(
        deviceId: String,
        deviceName: String,
    ): Outcome<Unit> =
        api.rename(
            RenameDeviceBody(
                deviceId = deviceId.take(MAX_DEVICE_ID_CHARS),
                deviceName = deviceName.take(MAX_META_CHARS),
            ),
        ).mapResult { }

    override suspend fun unregister(token: String): Outcome<Unit> =
        api.remove(RemoveDeviceBody(token)).mapResult { }

    private companion object {
        const val MAX_META_CHARS = 64

        const val MAX_DEVICE_ID_CHARS = 128
    }
}

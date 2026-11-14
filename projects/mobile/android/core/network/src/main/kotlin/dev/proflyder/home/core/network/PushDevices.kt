package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import okhttp3.Interceptor
import okhttp3.OkHttpClient

/**
 * Push device registration on the home scheduler.
 *
 * The registration lives next to the Connect API on the *same* scheduler base
 * URL and uses the same bearer token, so the app needs no fourth endpoint and
 * no second credential. The scheduler only stores the FCM token; delivery goes
 * through the separate push service.
 *
 * The row is keyed by [deviceId], not by the token: FCM rotates tokens, and a
 * rotation must update the same phone instead of adding a second one. The
 * owner-visible [deviceName] is server metadata and can change on its own.
 */
interface PushDevicesApi {
    /** Registers (or refreshes) this device's FCM token. */
    suspend fun register(
        deviceId: String,
        token: String,
        appVersion: String,
        deviceName: String,
    ): Outcome<Unit>

    /** Changes this device's owner-visible name without re-registering. */
    suspend fun rename(deviceId: String, deviceName: String): Outcome<Unit>

    /** Forgets a token that FCM rejected or that this device replaced. */
    suspend fun unregister(token: String): Outcome<Unit>
}

/** Wire format of `/v1/push/devices`. */
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

/** Plain JSON calls on the scheduler endpoint. */
internal interface PushApi {
    @retrofit2.http.POST("v1/push/devices")
    fun register(
        @retrofit2.http.Body body: RegisterDeviceBody,
    ): Outcome<PushDeviceResponseDto>

    @retrofit2.http.POST("v1/push/devices/remove")
    fun remove(
        @retrofit2.http.Body body: RemoveDeviceBody,
    ): Outcome<RemovedDeviceResponseDto>

    @retrofit2.http.POST("v1/push/devices/rename")
    fun rename(
        @retrofit2.http.Body body: RenameDeviceBody,
    ): Outcome<PushDeviceResponseDto>
}

/**
 * Retrofit implementation. It reuses [Target.SCHEDULER], so endpoints, the
 * bearer token, timeouts, TLS and the redirect policy are exactly the ones the
 * scheduler client already uses.
 */
class HttpPushDevicesClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    interceptors: List<Interceptor> = emptyList(),
) : PushDevicesApi {
    private val api: PushApi = HomeRetrofit.create(
        PushApi::class.java,
        settings,
        baseClient,
        Target.SCHEDULER,
        interceptors,
    )

    override suspend fun register(
        deviceId: String,
        token: String,
        appVersion: String,
        deviceName: String,
    ): Outcome<Unit> = withContext(ioDispatcher) {
        api.register(
            RegisterDeviceBody(
                deviceId = deviceId.take(MAX_DEVICE_ID_CHARS),
                token = token,
                appVersion = appVersion.take(MAX_META_CHARS),
                deviceName = deviceName.take(MAX_META_CHARS),
            ),
        ).mapResult { }
    }

    override suspend fun rename(
        deviceId: String,
        deviceName: String,
    ): Outcome<Unit> = withContext(ioDispatcher) {
        api.rename(
            RenameDeviceBody(
                deviceId = deviceId.take(MAX_DEVICE_ID_CHARS),
                deviceName = deviceName.take(MAX_META_CHARS),
            ),
        ).mapResult { }
    }

    override suspend fun unregister(token: String): Outcome<Unit> =
        withContext(ioDispatcher) { api.remove(RemoveDeviceBody(token)).mapResult { } }

    private companion object {
        /** Matches the scheduler's own metadata bound. */
        const val MAX_META_CHARS = 64

        /** Matches the scheduler's device identity bound. */
        const val MAX_DEVICE_ID_CHARS = 128
    }
}

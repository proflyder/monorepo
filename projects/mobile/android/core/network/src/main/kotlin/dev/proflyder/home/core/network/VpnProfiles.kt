package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.VpnOcservSettings
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnWireGuardSettings
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import okhttp3.Interceptor
import okhttp3.OkHttpClient

interface VpnProfileApi {
    suspend fun register(deviceId: String, publicKey: String, deviceName: String): Outcome<VpnProfile>

    suspend fun remove(deviceId: String): Outcome<Boolean>
}

@Serializable
internal data class VpnProfileBody(val deviceId: String, val publicKey: String, val deviceName: String)

@Serializable
internal data class VpnRemoveBody(val deviceId: String)

@Serializable
internal data class VpnInterfaceDto(val address: String, val dns: List<String> = emptyList(), val mtu: Int = 0)

@Serializable
internal data class VpnPeerDto(
    val publicKey: String,
    val endpoint: String,
    val allowedIps: List<String> = emptyList(),
    val persistentKeepalive: Int = 0,
)

@Serializable
internal data class VpnWireGuardDto(
    val `interface`: VpnInterfaceDto,
    val peer: VpnPeerDto,
    val splitAllowedIps: List<String> = emptyList(),
)

@Serializable
internal data class VpnOcservDto(
    val url: String = "",
    val group: String = "",
    val splitGroup: String = "",
    val authentication: List<String> = emptyList(),
)

@Serializable
internal data class VpnProfileDto(
    val deviceId: String,
    val deviceName: String = "",
    val wireguard: VpnWireGuardDto,
    val ocserv: VpnOcservDto = VpnOcservDto(),
    val createdUnix: String = "0",
    val updatedUnix: String = "0",
)

@Serializable
internal data class VpnProfileResponseDto(val profile: VpnProfileDto)

@Serializable
internal data class VpnRemovedResponseDto(val removed: Boolean = false)

internal interface VpnApi {
    @retrofit2.http.POST("v1/vpn/profile")
    fun register(@retrofit2.http.Body body: VpnProfileBody): Outcome<VpnProfileResponseDto>

    @retrofit2.http.POST("v1/vpn/profile/remove")
    fun remove(@retrofit2.http.Body body: VpnRemoveBody): Outcome<VpnRemovedResponseDto>
}

internal fun VpnProfileDto.toModel(): VpnProfile = VpnProfile(
    deviceId = deviceId,
    deviceName = deviceName,
    wireguard = VpnWireGuardSettings(
        address = wireguard.`interface`.address,
        dns = wireguard.`interface`.dns,
        mtu = wireguard.`interface`.mtu,
        serverPublicKey = wireguard.peer.publicKey,
        endpoint = wireguard.peer.endpoint,
        allowedIps = wireguard.peer.allowedIps,
        splitAllowedIps = wireguard.splitAllowedIps,
        persistentKeepalive = wireguard.peer.persistentKeepalive,
    ),
    ocserv = VpnOcservSettings(
        url = ocserv.url,
        group = ocserv.group,
        splitGroup = ocserv.splitGroup,
        authentication = ocserv.authentication,
    ),
    createdUnix = createdUnix.toLongOrNull() ?: 0,
    updatedUnix = updatedUnix.toLongOrNull() ?: 0,
)

class HttpVpnProfileClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    interceptors: List<Interceptor> = emptyList(),
) : VpnProfileApi {
    private val api: VpnApi = HomeRetrofit.create(
        VpnApi::class.java,
        settings,
        baseClient,
        Target.SCHEDULER,
        interceptors,
    )

    override suspend fun register(
        deviceId: String,
        publicKey: String,
        deviceName: String,
    ): Outcome<VpnProfile> = withContext(ioDispatcher) {
        api.register(
            VpnProfileBody(
                deviceId = deviceId.take(MAX_DEVICE_ID_CHARS),
                publicKey = publicKey,
                deviceName = deviceName.take(MAX_NAME_CHARS),
            ),
        ).mapResult { it.profile.toModel() }
    }

    override suspend fun remove(deviceId: String): Outcome<Boolean> = withContext(ioDispatcher) {
        api.remove(VpnRemoveBody(deviceId.take(MAX_DEVICE_ID_CHARS))).mapResult { it.removed }
    }

    private companion object {
        const val MAX_NAME_CHARS = 64

        const val MAX_DEVICE_ID_CHARS = 128
    }
}

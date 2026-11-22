package dev.proflyder.home.feature.vpn

import dev.proflyder.home.core.model.VpnProfile
import kotlinx.coroutines.flow.StateFlow

enum class VpnProtocol {
    WIREGUARD,
    OPENCONNECT,
}

enum class VpnRoute {
    ALL,
    HOME,
}

enum class VpnAuth {
    CERTIFICATE,
    PASSWORD_TOTP,
}

data class VpnSettings(
    val route: VpnRoute = VpnRoute.ALL,
    val autoConnect: Boolean = false,
    val auth: VpnAuth = VpnAuth.CERTIFICATE,
    val certificateName: String = "",
)

enum class VpnProfileProblem {
    NOT_CONFIGURED,
    OFFLINE,
    UNAUTHORIZED,
    INVALID,
    CONFLICT,
    LIMIT,
    UNAVAILABLE,
    SERVER,
}

sealed interface VpnProfileState {
    data object Missing : VpnProfileState

    data object Loading : VpnProfileState

    data class Ready(val profile: VpnProfile) : VpnProfileState

    data class Failed(val problem: VpnProfileProblem, val profile: VpnProfile? = null) : VpnProfileState
}

enum class VpnConnectProblem {
    NO_PROFILE,
    PERMISSION,
    TUNNEL,
    NO_EXTERNAL_CLIENT,
}

sealed interface VpnConnection {
    data object Disconnected : VpnConnection

    data class Connecting(val protocol: VpnProtocol) : VpnConnection

    data class Connected(
        val protocol: VpnProtocol,
        val sinceMillis: Long,
        val rxBytes: Long = 0,
        val txBytes: Long = 0,
    ) : VpnConnection

    data object FallbackRequired : VpnConnection

    data object NotNeededAtHome : VpnConnection

    data class Failed(val problem: VpnConnectProblem) : VpnConnection
}

val VpnConnection.isActive: Boolean
    get() = this is VpnConnection.Connecting || this is VpnConnection.Connected

interface VpnSettingsStore {
    val settings: StateFlow<VpnSettings>

    suspend fun load()

    suspend fun setRoute(route: VpnRoute)

    suspend fun setAutoConnect(enabled: Boolean)

    suspend fun setAuth(auth: VpnAuth)

    suspend fun setCertificateName(name: String)
}

interface VpnProfiles {
    val state: StateFlow<VpnProfileState>

    suspend fun load()

    suspend fun ensure(): VpnProfile?

    suspend fun reissue(): Boolean

    suspend fun forget(): Boolean
}

interface VpnController {
    val connection: StateFlow<VpnConnection>

    fun connect()

    fun disconnect()
}

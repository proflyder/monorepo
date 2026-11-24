package dev.proflyder.home.feature.vpn

import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnProtocolId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

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
    val connectOnOpen: Boolean = true,
    val permissionAsked: Boolean = false,
    val auth: VpnAuth = VpnAuth.CERTIFICATE,
    val certificateName: String = "",
)

enum class VpnProfileProblem {
    NOT_CONFIGURED,
    OFFLINE,
    UNAUTHORIZED,
    NO_ACCESS,
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

val VpnProfileState.knownProfile: VpnProfile?
    get() = when (this) {
        is VpnProfileState.Ready -> profile
        is VpnProfileState.Failed -> profile
        else -> null
    }

enum class VpnConnectProblem {
    NO_PROFILE,
    PERMISSION,
    TUNNEL,
    NO_CLIENT,
}

data class VpnTransportInfo(
    val id: VpnProtocolId,
    val labelRes: Int,
    val external: Boolean,
)

data class VpnTraffic(val rxBytes: Long, val txBytes: Long)

sealed interface VpnConnection {
    data object Disconnected : VpnConnection

    data class Connecting(val transport: VpnTransportInfo) : VpnConnection

    data class Connected(
        val transport: VpnTransportInfo,
        val sinceMillis: Long,
        val verified: Boolean,
    ) : VpnConnection

    data class FallbackRequired(val options: List<VpnTransportInfo>) : VpnConnection

    data object NotNeededAtHome : VpnConnection

    data class Failed(val problem: VpnConnectProblem) : VpnConnection
}

val VpnConnection.isActive: Boolean
    get() = this is VpnConnection.Connecting || this is VpnConnection.Connected

interface VpnSettingsStore {
    val settings: StateFlow<VpnSettings?>

    suspend fun setRoute(route: VpnRoute)

    suspend fun setAutoConnect(enabled: Boolean)

    suspend fun setConnectOnOpen(enabled: Boolean)

    suspend fun markPermissionAsked()

    suspend fun setAuth(auth: VpnAuth)

    suspend fun setCertificateName(name: String)
}

interface VpnProfiles {
    val state: StateFlow<VpnProfileState>

    suspend fun ensure(): VpnProfile?

    suspend fun reissue(): Boolean

    suspend fun forget(): Boolean
}

interface VpnController {
    val connection: StateFlow<VpnConnection>

    val traffic: Flow<VpnTraffic?>

    fun availableTransports(): List<VpnTransportInfo>

    fun connect()

    fun connectVia(transport: VpnProtocolId)

    fun reconnect()

    fun disconnect()
}

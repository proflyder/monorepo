package dev.proflyder.home.feature.vpn.transport

import dev.proflyder.home.core.model.TransportSettings
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.VpnTraffic
import dev.proflyder.home.feature.vpn.VpnTransportInfo
import kotlin.time.Duration
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf

internal data class TunnelRequest(
    val profile: VpnProfile,
    val settings: TransportSettings,
    val route: VpnRoute,
    val atHome: Boolean?,
)

internal sealed interface TransportStatus {
    data object Down : TransportStatus

    data object Pending : TransportStatus

    data class Up(val verified: Boolean) : TransportStatus
}

internal interface VpnTransport {
    val info: VpnTransportInfo

    val order: Int

    val needsSystemPermission: Boolean

    val handshakeTimeout: Duration

    val status: Flow<TransportStatus>

    val traffic: Flow<VpnTraffic?> get() = flowOf(null)

    val systemStarts: Flow<Unit> get() = emptyFlow()

    fun installed(): Boolean = true

    fun needsRestart(current: TunnelRequest, next: TunnelRequest): Boolean = false

    suspend fun up(request: TunnelRequest): Boolean

    suspend fun down()
}

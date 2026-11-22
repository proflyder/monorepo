package dev.proflyder.home.feature.vpn.presentation

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.runtime.tickerFlow
import dev.proflyder.home.feature.vpn.NetworkEnvironment
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.VpnSettings
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.VpnTraffic
import dev.proflyder.home.feature.vpn.atHome
import dev.zacsweers.metro.Inject
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map

internal data class VpnSnapshot(
    val connection: VpnConnection,
    val traffic: VpnTraffic?,
    val profile: VpnProfileState,
    val settings: VpnSettings,
    val atHome: Boolean?,
    val nowMillis: Long,
)

@Inject
internal class VpnStateFeed(
    private val controller: VpnController,
    private val profiles: VpnProfiles,
    private val settings: VpnSettingsStore,
    private val environment: NetworkEnvironment,
    private val clock: Clock,
) {
    fun snapshots(): Flow<VpnSnapshot> = combine(
        controller.connection,
        controller.traffic,
        profiles.state,
        settings.settings,
        environment.state,
    ) { connection, traffic, profile, value, network ->
        VpnSnapshot(connection, traffic, profile, value ?: VpnSettings(), network.atHome, 0)
    }.combine(clockTicks()) { snapshot, now -> snapshot.copy(nowMillis = now) }

    private fun clockTicks(): Flow<Long> = tickerFlow(TICK).map { clock.wallMillis() }

    private companion object {
        val TICK = 1.seconds
    }
}

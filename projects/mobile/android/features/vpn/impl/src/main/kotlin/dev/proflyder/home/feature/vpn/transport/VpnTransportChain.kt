package dev.proflyder.home.feature.vpn.transport

import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.feature.vpn.VpnTransportInfo
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.merge

@SingleIn(AppScope::class)
@Inject
internal class VpnTransportChain(transports: Set<VpnTransport>) : SystemVpnStarts {
    private val ordered: List<VpnTransport> = transports.sortedBy { it.order }

    override val starts: Flow<Unit> = ordered.map { it.systemStarts }.merge()

    fun find(id: VpnProtocolId): VpnTransport? = ordered.firstOrNull { it.info.id == id }

    fun automatic(profile: VpnProfile): List<VpnTransport> =
        ordered.filter { !it.info.external && profile.settingsOf(it.info.id) != null }

    fun fallbacks(profile: VpnProfile): List<VpnTransportInfo> =
        ordered.filter { it.info.external && profile.settingsOf(it.info.id) != null }.map { it.info }

    fun available(): List<VpnTransportInfo> = ordered.filter { it.installed() }.map { it.info }
}

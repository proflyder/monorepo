package dev.proflyder.home.feature.vpn.tunnel

import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.feature.vpn.VpnProtocol
import dev.proflyder.home.feature.vpn.VpnRoute

data class VpnTraffic(val rxBytes: Long = 0, val txBytes: Long = 0)

interface VpnTransport {
    val protocol: VpnProtocol

    suspend fun up(profile: VpnProfile, route: VpnRoute): Boolean

    suspend fun down()

    fun latestHandshakeMillis(): Long

    fun traffic(): VpnTraffic
}

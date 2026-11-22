package dev.proflyder.home.feature.vpn.tunnel

import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.feature.vpn.FakeClock
import dev.proflyder.home.feature.vpn.FakeEnvironment
import dev.proflyder.home.feature.vpn.FakePermission
import dev.proflyder.home.feature.vpn.FakeProfiles
import dev.proflyder.home.feature.vpn.FakeSettings
import dev.proflyder.home.feature.vpn.FakeTransport
import dev.proflyder.home.feature.vpn.Handshake
import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.VpnConnectProblem
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnSamples
import dev.proflyder.home.feature.vpn.transport.VpnTransportChain
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ChainVpnControllerTest {
    private val away = FakeEnvironment(HomeNetworkState.Away(setOf(1L)))

    private fun profileWith(vararg ids: String) = FakeProfiles(
        VpnSamples.profile.copy(transports = ids.associate { VpnProtocolId(it) to VpnSamples.wireGuard }),
    )

    private fun controller(
        scope: CoroutineScope,
        transports: List<FakeTransport>,
        profiles: FakeProfiles,
        environment: FakeEnvironment = away,
        permission: FakePermission = FakePermission(),
    ) = ChainVpnController(
        VpnTransportChain(transports.toSet()),
        profiles,
        FakeSettings(),
        environment,
        permission,
        FakeClock,
        scope,
    ).also { it.start() }

    @Test
    fun aThirdTransportJoinsTheChainWithoutChangesToTheController() = runTest {
        val silent = FakeTransport("primary", order = 0, handshake = Handshake.SILENT)
        val third = FakeTransport("third", order = 5, handshake = Handshake.ANSWERS)
        val vpn = controller(backgroundScope, listOf(third, silent), profileWith("primary", "third"))
        vpn.connect()
        runCurrent()
        assertEquals(VpnConnection.Connecting(silent.info), vpn.connection.value)
        advanceTimeBy(silent.handshakeTimeout.inWholeMilliseconds + 1)
        runCurrent()
        assertEquals(VpnConnection.Connected(third.info, FakeClock.wallMillis(), verified = true), vpn.connection.value)
        assertEquals(1, silent.downs)
    }

    @Test
    fun silentChainOffersOnlyExternalFallbacksAwayAndNothingAtHome() = runTest {
        val silent = FakeTransport("primary", order = 0, handshake = Handshake.SILENT)
        val external = FakeTransport("external", order = 9, handshake = Handshake.ANSWERS, external = true)
        val awayVpn = controller(backgroundScope, listOf(silent, external), profileWith("primary", "external"))
        awayVpn.connect()
        settle()
        assertEquals(VpnConnection.FallbackRequired(listOf(external.info)), awayVpn.connection.value)
        assertEquals(0, external.requests.size)

        val home = FakeEnvironment(HomeNetworkState.Home(setOf(1L)))
        val homeVpn = controller(backgroundScope, listOf(silent), profileWith("primary"), home)
        homeVpn.connect()
        settle()
        assertEquals(VpnConnection.NotNeededAtHome, homeVpn.connection.value)
    }

    @Test
    fun externalTransportStartsOnlyOnRequestAndIsReportedUnverified() = runTest {
        val external = FakeTransport("external", order = 9, handshake = Handshake.ANSWERS, external = true)
        val vpn = controller(backgroundScope, listOf(external), profileWith("external"))
        vpn.connectVia(external.info.id)
        runCurrent()
        assertEquals(VpnConnection.Connected(external.info, FakeClock.wallMillis(), verified = false), vpn.connection.value)
    }

    @Test
    fun commandsRunInOrderSoADisconnectDuringConnectWins() = runTest {
        val silent = FakeTransport("primary", order = 0, handshake = Handshake.SILENT)
        val vpn = controller(backgroundScope, listOf(silent), profileWith("primary"))
        vpn.connect()
        runCurrent()
        vpn.connect()
        vpn.disconnect()
        settle()
        assertEquals(VpnConnection.Disconnected, vpn.connection.value)
        assertEquals(1, silent.requests.size)
        assertEquals(1, silent.downs)
    }

    @Test
    fun networkChangeRestartsATransportThatAsksForIt() = runTest {
        val environment = FakeEnvironment(HomeNetworkState.Away(setOf(1L)))
        val primary = FakeTransport("primary", order = 0, handshake = Handshake.ANSWERS, restartOnNetworkChange = true)
        val vpn = controller(backgroundScope, listOf(primary), profileWith("primary"), environment)
        vpn.connect()
        runCurrent()
        environment.state.value = HomeNetworkState.Home(setOf(2L))
        runCurrent()
        assertEquals(listOf(false, true), primary.requests.map { it.atHome })
        assertEquals(VpnConnection.Connected(primary.info, FakeClock.wallMillis(), verified = true), vpn.connection.value)
    }

    @Test
    fun missingPermissionOrProfileFailsWithoutStartingATunnel() = runTest {
        val primary = FakeTransport("primary", order = 0, handshake = Handshake.ANSWERS)
        val noPermission = controller(backgroundScope, listOf(primary), profileWith("primary"), permission = FakePermission(false))
        noPermission.connect()
        runCurrent()
        assertEquals(VpnConnection.Failed(VpnConnectProblem.PERMISSION), noPermission.connection.value)
        val noProfile = controller(backgroundScope, listOf(primary), FakeProfiles(null))
        noProfile.connect()
        runCurrent()
        assertEquals(VpnConnection.Failed(VpnConnectProblem.NO_PROFILE), noProfile.connection.value)
        assertEquals(0, primary.requests.size)
    }


    private fun TestScope.settle() {
        advanceTimeBy(SETTLE_MILLIS)
        runCurrent()
    }

    private companion object {
        const val SETTLE_MILLIS = 60_000L
    }
}

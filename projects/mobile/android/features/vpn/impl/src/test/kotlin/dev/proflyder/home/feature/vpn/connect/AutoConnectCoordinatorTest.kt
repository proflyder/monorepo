package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.feature.vpn.FakeController
import dev.proflyder.home.feature.vpn.FakeEnvironment
import dev.proflyder.home.feature.vpn.FakeForeground
import dev.proflyder.home.feature.vpn.FakePermission
import dev.proflyder.home.feature.vpn.FakeProfiles
import dev.proflyder.home.feature.vpn.FakeSettings
import dev.proflyder.home.feature.vpn.FakeSystemStarts
import dev.proflyder.home.feature.vpn.FakeTransport
import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnSettings
import dev.proflyder.home.feature.vpn.tunnel.ConnectOrigin
import dev.proflyder.home.core.testing.FakeAppLogger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AutoConnectCoordinatorTest {
    private val controller = FakeController()
    private val settings = FakeSettings()
    private val environment = FakeEnvironment()
    private val foreground = FakeForeground(foreground = true)
    private val permission = FakePermission()
    private val userIntent = UserVpnIntent(environment)
    private val user = UserVpnCommands(controller, userIntent)
    private val systemStarts = FakeSystemStarts()
    private val logger = FakeAppLogger()

    private fun coordinator(scope: CoroutineScope) = AutoConnectCoordinator(
        controller,
        controller,
        permission,
        settings,
        FakeProfiles(),
        environment,
        foreground,
        userIntent,
        systemStarts,
        AutoConnectJournal(logger),
        scope,
    ).also { it.start() }

    private val connected = VpnConnection.Connected(FakeTransport.info("primary"), 0, verified = true)

    @Test
    fun opensAwayFromHomeConnectOnceAndRespectAManualDisconnect() = runTest {
        val subject = coordinator(backgroundScope)
        runCurrent()
        assertEquals(HomeRoute.CHECKING, subject.route.value)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        assertEquals(listOf("connect"), controller.commands)
        assertEquals(HomeRoute.CONNECTING, subject.route.value)
        controller.connection.value = VpnConnection.Connected(FakeTransport.info("primary"), 0, verified = true)
        runCurrent()
        assertEquals(HomeRoute.READY, subject.route.value)
        controller.disconnect()
        runCurrent()
        assertEquals(listOf("connect", "disconnect"), controller.commands)
    }

    @Test
    fun aNewNetworkOrANewForegroundIsANewDecision() = runTest {
        coordinator(backgroundScope)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        controller.disconnect()
        foreground.move(false)
        runCurrent()
        foreground.move(true)
        runCurrent()
        environment.state.value = HomeNetworkState.Away(setOf(2L))
        controller.disconnect()
        runCurrent()
        assertEquals(3, controller.commands.count { it == "connect" })
    }

    @Test
    fun autoConnectWorksInTheBackgroundAndDisconnectsAtHome() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true, connectOnOpen = false)
        foreground.move(false)
        coordinator(backgroundScope)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        controller.connection.value = VpnConnection.Connected(FakeTransport.info("primary"), 0, verified = true)
        environment.state.value = HomeNetworkState.Home(setOf(2L))
        runCurrent()
        assertEquals(listOf("connect", "disconnect"), controller.commands)
    }

    @Test
    fun backgroundWithoutAutoConnectLeavesTheTunnelAlone() = runTest {
        foreground.move(false)
        coordinator(backgroundScope)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        assertTrue(controller.commands.isEmpty())
    }

    @Test
    fun missingPermissionIsAskedOnceInTheForeground() = runTest {
        permission.allowed = false
        val subject = coordinator(backgroundScope)
        val requests = mutableListOf<Unit>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { subject.permissionRequests.toList(requests) }
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        environment.state.value = HomeNetworkState.Away(setOf(2L))
        runCurrent()
        assertEquals(1, requests.size)
        assertTrue(settings.settings.value?.permissionAsked == true)
        assertTrue(controller.commands.isEmpty())
    }

    @Test
    fun aManualConnectAtHomeIsNotTornDownByAutoConnect() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true)
        environment.state.value = HomeNetworkState.Home(setOf(1L))
        coordinator(backgroundScope)
        runCurrent()
        user.connect()
        runCurrent()
        controller.connection.value = connected
        runCurrent()
        foreground.move(false)
        runCurrent()
        foreground.move(true)
        runCurrent()
        assertEquals(listOf("connect"), controller.commands)
    }

    @Test
    fun aManualDisconnectAwayIsNotRaisedAgainInTheSameNetwork() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true)
        coordinator(backgroundScope)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        controller.connection.value = connected
        runCurrent()
        user.disconnect()
        runCurrent()
        foreground.move(false)
        runCurrent()
        foreground.move(true)
        runCurrent()
        assertEquals(listOf("connect", "disconnect"), controller.commands)
    }

    @Test
    fun aNewNetworkHandsTheDecisionBackToThePolicy() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true)
        environment.state.value = HomeNetworkState.Home(setOf(1L))
        coordinator(backgroundScope)
        runCurrent()
        user.connect()
        controller.connection.value = connected
        runCurrent()
        environment.state.value = HomeNetworkState.Home(setOf(2L))
        runCurrent()
        assertEquals(listOf("connect", "disconnect"), controller.commands)
        assertEquals(null, userIntent.choice.value)
    }

    @Test
    fun aSystemStartAtHomeNeverRaisesTheTunnel() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true)
        foreground.move(false)
        environment.state.value = HomeNetworkState.Home(setOf(1L))
        coordinator(backgroundScope)
        runCurrent()
        systemStarts.starts.emit(Unit)
        runCurrent()
        assertTrue(controller.commands.isEmpty())
        assertTrue(logger.entries.any { "source=SYSTEM_START action=NONE reason=AT_HOME" in it.message })
    }

    @Test
    fun aSystemStartInTheBackgroundNeedsAutoConnect() = runTest {
        foreground.move(false)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        coordinator(backgroundScope)
        runCurrent()
        systemStarts.starts.emit(Unit)
        runCurrent()
        assertTrue(controller.commands.isEmpty())
    }

    @Test
    fun aSystemStartRespectsAManualDisconnectInTheSameNetwork() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true)
        foreground.move(false)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        coordinator(backgroundScope)
        runCurrent()
        user.disconnect()
        runCurrent()
        systemStarts.starts.emit(Unit)
        runCurrent()
        assertEquals(listOf("connect", "disconnect"), controller.commands)
        assertTrue(logger.entries.any { "source=SYSTEM_START action=NONE reason=USER_CHOICE" in it.message })
    }

    @Test
    fun aSystemStartWaitsForTheEnvironmentAndGivesUpWhenItStaysUnknown() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true)
        foreground.move(false)
        coordinator(backgroundScope)
        runCurrent()
        systemStarts.starts.emit(Unit)
        advanceTimeBy(30_000)
        runCurrent()
        assertTrue(controller.commands.isEmpty())
        assertTrue(logger.entries.any { "source=SYSTEM_START action=NONE reason=ENVIRONMENT_UNKNOWN" in it.message })
        systemStarts.starts.emit(Unit)
        advanceTimeBy(2_000)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        assertEquals(listOf(ConnectOrigin.AUTO_POLICY), controller.origins)
        assertTrue(logger.entries.any { "source=SYSTEM_START action=NONE reason=ENVIRONMENT_UNKNOWN" in it.message })
    }

    @Test
    fun anUnknownEnvironmentIsAwaitedWithATimeoutWithoutConnecting() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true)
        val subject = coordinator(backgroundScope)
        runCurrent()
        assertEquals(HomeRoute.CHECKING, subject.route.value)
        advanceTimeBy(16_000)
        runCurrent()
        assertEquals(HomeRoute.READY, subject.route.value)
        assertTrue(controller.commands.isEmpty())
        assertTrue(logger.entries.any { "source=ENVIRONMENT_TIMEOUT action=NONE reason=ENVIRONMENT_UNKNOWN" in it.message })
    }

    @Test
    fun aFailedAutoAttemptIsNotRetriedInTheSameNetwork() = runTest {
        settings.settings.value = VpnSettings(autoConnect = true)
        foreground.move(false)
        coordinator(backgroundScope)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        controller.connection.value = VpnConnection.FallbackRequired(emptyList())
        runCurrent()
        foreground.move(false)
        runCurrent()
        assertEquals(listOf(ConnectOrigin.AUTO_POLICY), controller.origins)
    }

    @Test
    fun decisionsAreLoggedWithReasonEnvironmentAndIntent() = runTest {
        coordinator(backgroundScope)
        environment.state.value = HomeNetworkState.Away(setOf(1L))
        runCurrent()
        val line = logger.entries.last { "action=CONNECT" in it.message }.message
        assertTrue(line, "reason=AWAY_REQUESTED environment=Away foreground=true" in line)
        assertTrue(line, "wish=null" in line)
    }
}

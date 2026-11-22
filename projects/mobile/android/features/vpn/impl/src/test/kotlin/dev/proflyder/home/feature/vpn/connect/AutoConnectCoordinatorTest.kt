package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.feature.vpn.FakeController
import dev.proflyder.home.feature.vpn.FakeEnvironment
import dev.proflyder.home.feature.vpn.FakeForeground
import dev.proflyder.home.feature.vpn.FakePermission
import dev.proflyder.home.feature.vpn.FakeProfiles
import dev.proflyder.home.feature.vpn.FakeSettings
import dev.proflyder.home.feature.vpn.FakeTransport
import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnSettings
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
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

    private fun coordinator(scope: CoroutineScope) = AutoConnectCoordinator(
        controller,
        permission,
        settings,
        FakeProfiles(),
        environment,
        foreground,
        scope,
    ).also { it.start() }

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
}

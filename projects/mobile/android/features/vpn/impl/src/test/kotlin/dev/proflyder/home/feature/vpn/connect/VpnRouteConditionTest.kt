package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.feature.vpn.FakeTransport
import dev.proflyder.home.feature.vpn.VpnConnection
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VpnRouteConditionTest {
    private val connecting = VpnConnection.Connecting(FakeTransport.info("primary"))
    private val connected = VpnConnection.Connected(FakeTransport.info("primary"), 0, verified = true)

    @Test
    fun requestsWaitWhileTheTunnelIsSwitching() {
        assertFalse(routeOpen(HomeRoute.READY, connecting, switching = false))
        assertFalse(routeOpen(HomeRoute.READY, connected, switching = true))
        assertFalse(routeOpen(HomeRoute.READY, VpnConnection.Disconnected, switching = true))
        assertFalse(routeOpen(HomeRoute.CONNECTING, VpnConnection.Disconnected, switching = false))
    }

    @Test
    fun theRouteOpensOnceTheTunnelSettles() {
        assertTrue(routeOpen(HomeRoute.READY, connected, switching = false))
        assertTrue(routeOpen(HomeRoute.READY, VpnConnection.Disconnected, switching = false))
    }
}

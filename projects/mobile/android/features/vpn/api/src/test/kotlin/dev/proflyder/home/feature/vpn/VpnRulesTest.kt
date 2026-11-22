package dev.proflyder.home.feature.vpn

import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.VpnOcservSettings
import dev.proflyder.home.core.model.VpnWireGuardSettings
import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VpnRulesTest {
    private val wireguard = VpnWireGuardSettings(
        address = "10.77.2.2/32",
        dns = listOf("192.0.2.1"),
        mtu = 1380,
        serverPublicKey = "server-key",
        endpoint = "vpn.example.com:51820",
        allowedIps = listOf("0.0.0.0/0"),
        splitAllowedIps = listOf("192.0.2.0/24"),
        persistentKeepalive = 25,
        lanEndpoint = "192.0.2.29:51820",
    )

    private val ocserv = VpnOcservSettings("https://vpn.example.com", "full", "lan", listOf("certificate"))

    @Test
    fun homeRouteUsesSplitAllowedIpsAndFallsBackToFullWhenEmpty() {
        assertEquals(listOf("192.0.2.0/24"), VpnRoutes.allowedIps(wireguard, VpnRoute.HOME))
        assertEquals(listOf("0.0.0.0/0"), VpnRoutes.allowedIps(wireguard, VpnRoute.ALL))
        val noSplit = wireguard.copy(splitAllowedIps = emptyList())
        assertEquals(listOf("0.0.0.0/0"), VpnRoutes.allowedIps(noSplit, VpnRoute.HOME))
    }

    @Test
    fun configTextCarriesTheProfileAndTheChosenRoute() {
        val text = WireGuardConfigText.render(wireguard, "private-key", VpnRoute.HOME)
        assertTrue(text.contains("PrivateKey = private-key"))
        assertTrue(text.contains("Address = 10.77.2.2/32"))
        assertTrue(text.contains("DNS = 192.0.2.1"))
        assertTrue(text.contains("MTU = 1380"))
        assertTrue(text.contains("Endpoint = vpn.example.com:51820"))
        assertTrue(text.contains("AllowedIPs = 192.0.2.0/24"))
        assertTrue(text.contains("PersistentKeepalive = 25"))
    }

    @Test
    fun homeNetworkUsesTheLanEndpointAndEverywhereElseThePublicOne() {
        assertEquals("192.0.2.29:51820", VpnEndpoints.select(wireguard, atHome = true))
        assertEquals("vpn.example.com:51820", VpnEndpoints.select(wireguard, atHome = false))
        assertEquals("vpn.example.com:51820", VpnEndpoints.select(wireguard, atHome = null))
        val noLan = wireguard.copy(lanEndpoint = "")
        assertEquals("vpn.example.com:51820", VpnEndpoints.select(noLan, atHome = true))
        val text = WireGuardConfigText.render(wireguard, "private-key", VpnRoute.ALL, atHome = true)
        assertTrue(text.contains("Endpoint = 192.0.2.29:51820"))
    }

    @Test
    fun networkChangeSwitchesTheEndpointOfAnActiveTunnelOnly() {
        val lan = "192.0.2.29:51820"
        val public = "vpn.example.com:51820"
        assertTrue(HomeNetworkRules.shouldSwitchEndpoint(false, true, true, public, lan))
        assertTrue(HomeNetworkRules.shouldSwitchEndpoint(false, false, true, lan, public))
        assertFalse(HomeNetworkRules.shouldSwitchEndpoint(false, true, true, lan, lan))
        assertFalse(HomeNetworkRules.shouldSwitchEndpoint(false, true, false, public, lan))
        assertFalse(HomeNetworkRules.shouldSwitchEndpoint(true, true, true, public, lan))
    }

    @Test
    fun silentWireGuardNeverStartsTheExternalClient() {
        assertEquals(VpnConnection.FallbackRequired, HandshakePolicy.afterWireGuardTimeout(atHome = false))
        assertEquals(VpnConnection.FallbackRequired, HandshakePolicy.afterWireGuardTimeout(atHome = null))
        assertEquals(VpnConnection.NotNeededAtHome, HandshakePolicy.afterWireGuardTimeout(atHome = true))
        assertFalse(HandshakePolicy.afterWireGuardTimeout(atHome = false).isActive)
    }

    @Test
    fun handshakeFallsBackOnlyAfterTenSecondsWithoutHandshake() {
        assertEquals(HandshakeVerdict.WAIT, HandshakePolicy.verdict(0, 9_999, 0))
        assertEquals(HandshakeVerdict.FALLBACK, HandshakePolicy.verdict(0, 10_000, 0))
        assertEquals(HandshakeVerdict.CONNECTED, HandshakePolicy.verdict(0, 20_000, 15_000))
    }

    @Test
    fun homeAddressesArePrivateOnly() {
        assertTrue(HomeNetworkRules.isHomeAddress(InetAddress.getByName("192.168.1.10")))
        assertTrue(HomeNetworkRules.isHomeAddress(InetAddress.getByName("10.0.0.2")))
        assertTrue(HomeNetworkRules.isHomeAddress(InetAddress.getByName("fd00::1")))
        assertFalse(HomeNetworkRules.isHomeAddress(InetAddress.getByName("203.0.113.5")))
        assertFalse(HomeNetworkRules.isHomeAddress(InetAddress.getByName("2001:db8::1")))
    }

    @Test
    fun autoConnectActsOnlyWhenEnabled() {
        assertTrue(HomeNetworkRules.shouldConnect(autoConnect = true, atHome = false, active = false))
        assertFalse(HomeNetworkRules.shouldConnect(autoConnect = false, atHome = false, active = false))
        assertTrue(HomeNetworkRules.shouldDisconnect(autoConnect = true, atHome = true, active = true))
        assertFalse(HomeNetworkRules.shouldDisconnect(autoConnect = true, atHome = false, active = true))
    }

    @Test
    fun serverCodesMapToProblems() {
        assertEquals(VpnProfileProblem.CONFLICT, VpnProblems.of(RemoteFailure.Server("http_409", "")))
        assertEquals(VpnProfileProblem.LIMIT, VpnProblems.of(RemoteFailure.Server("http_429", "")))
        assertEquals(VpnProfileProblem.UNAVAILABLE, VpnProblems.of(RemoteFailure.Server("http_503", "")))
        assertEquals(VpnProfileProblem.INVALID, VpnProblems.of(RemoteFailure.Server("http_400", "")))
        assertEquals(VpnProfileProblem.OFFLINE, VpnProblems.of(RemoteFailure.Timeout))
        assertEquals(VpnProfileProblem.UNAUTHORIZED, VpnProblems.of(RemoteFailure.Unauthorized))
    }

    @Test
    fun openConnectLinkCarriesHostAndGroupForTheRoute() {
        assertEquals(
            "anyconnect://connect/?name=Home&host=vpn.example.com%2Flan",
            OpenConnectLink.connect(ocserv, VpnRoute.HOME, "Home"),
        )
        assertEquals(
            "anyconnect://connect/?name=Home&host=vpn.example.com%2Ffull",
            OpenConnectLink.connect(ocserv, VpnRoute.ALL, "Home"),
        )
        assertNull(OpenConnectLink.connect(ocserv.copy(url = ""), VpnRoute.ALL, "Home"))
    }

    @Test
    fun uapiStatsSumPeersAndConvertTheHandshake() {
        val text = "private_key=abc\npublic_key=def\nrx_bytes=100\ntx_bytes=40\n" +
            "last_handshake_time_sec=1700000000\nlast_handshake_time_nsec=5000000\nerrno=0\n"
        val stats = WireGuardUapi.parse(text)
        assertEquals(100, stats.rxBytes)
        assertEquals(40, stats.txBytes)
        assertEquals(1_700_000_000_005L, stats.latestHandshakeMillis)
        assertEquals(0, WireGuardUapi.parse(null).latestHandshakeMillis)
    }
}

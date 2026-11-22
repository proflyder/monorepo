package dev.proflyder.home.feature.vpn.transport

import com.wireguard.android.backend.GoBackend
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.VpnSamples
import dev.proflyder.home.feature.vpn.transport.openconnect.OpenConnectLink
import dev.proflyder.home.feature.vpn.transport.openconnect.OpenConnectSettings
import dev.proflyder.home.feature.vpn.transport.wireguard.GoBackendUapi
import dev.proflyder.home.feature.vpn.transport.wireguard.WireGuardConfigText
import dev.proflyder.home.feature.vpn.transport.wireguard.WireGuardSettings
import dev.proflyder.home.feature.vpn.transport.wireguard.WireGuardUapi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TransportProtocolsTest {
    private val wireGuard = checkNotNull(WireGuardSettings.of(VpnSamples.wireGuard))
    private val ocserv = checkNotNull(OpenConnectSettings.of(VpnSamples.ocserv))

    @Test
    fun homeRouteUsesSplitAllowedIpsAndFallsBackToFullWhenEmpty() {
        assertEquals(listOf("192.0.2.0/24"), wireGuard.allowedIps(VpnRoute.HOME))
        assertEquals(listOf("0.0.0.0/0"), wireGuard.allowedIps(VpnRoute.ALL))
        assertEquals(listOf("0.0.0.0/0"), wireGuard.copy(splitAllowedIps = emptyList()).allowedIps(VpnRoute.HOME))
    }

    @Test
    fun configTextCarriesTheProfileTheRouteAndTheEndpointForTheNetwork() {
        val text = WireGuardConfigText.render(wireGuard, "private-key", VpnRoute.HOME, atHome = false)
        assertTrue(text.contains("PrivateKey = private-key"))
        assertTrue(text.contains("Address = 10.77.2.2/32"))
        assertTrue(text.contains("DNS = 192.0.2.1"))
        assertTrue(text.contains("MTU = 1380"))
        assertTrue(text.contains("Endpoint = vpn.example.com:51820"))
        assertTrue(text.contains("AllowedIPs = 192.0.2.0/24"))
        assertTrue(text.contains("PersistentKeepalive = 25"))
        val home = WireGuardConfigText.render(wireGuard, "private-key", VpnRoute.ALL, atHome = true)
        assertTrue(home.contains("Endpoint = 192.0.2.29:51820"))
    }

    @Test
    fun homeNetworkUsesTheLanEndpointAndEverywhereElseThePublicOne() {
        assertEquals("192.0.2.29:51820", wireGuard.endpoint(atHome = true))
        assertEquals("vpn.example.com:51820", wireGuard.endpoint(atHome = false))
        assertEquals("vpn.example.com:51820", wireGuard.endpoint(atHome = null))
        assertEquals("vpn.example.com:51820", wireGuard.copy(peer = wireGuard.peer.copy(lanEndpoint = "")).endpoint(true))
    }

    @Test
    fun openConnectLinkCarriesHostAndGroupForTheRoute() {
        assertEquals("anyconnect://connect/?name=Home&host=vpn.example.com%2Flan", OpenConnectLink.connect(ocserv, VpnRoute.HOME, "Home"))
        assertEquals("anyconnect://connect/?name=Home&host=vpn.example.com%2Ffull", OpenConnectLink.connect(ocserv, VpnRoute.ALL, "Home"))
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

    @Test
    fun backendStillHasThePrivateMembersTheStatisticsAdapterReads() {
        assertNotNull(GoBackend::class.java.getDeclaredField(GoBackendUapi.HANDLE_FIELD))
        assertNotNull(GoBackend::class.java.getDeclaredMethod(GoBackendUapi.CONFIG_METHOD, Int::class.javaPrimitiveType))
    }
}

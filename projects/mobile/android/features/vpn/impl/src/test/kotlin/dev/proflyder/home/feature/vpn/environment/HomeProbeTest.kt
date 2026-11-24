package dev.proflyder.home.feature.vpn.environment

import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Test

class HomeProbeTest {
    private val lan = InetAddress.getByName("192.168.3.27")
    private val public = InetAddress.getByName("203.0.113.5")

    @Test
    fun noResolvedAddressMeansTheNetworkIsNotClassified() {
        assertEquals(HomeProbe.UNRESOLVED, homeProbe(emptyList()) { true })
    }

    @Test
    fun aReachablePrivateAddressIsHome() {
        assertEquals(HomeProbe.REACHED, homeProbe(listOf(lan, public)) { it == lan })
    }

    @Test
    fun aResolvedAddressThatDoesNotAnswerIsAway() {
        assertEquals(HomeProbe.RESOLVED, homeProbe(listOf(lan, public)) { false })
    }

    @Test
    fun aPublicAddressAloneIsNeverHome() {
        assertEquals(HomeProbe.RESOLVED, homeProbe(listOf(public)) { true })
    }

    @Test
    fun theBestProbeAcrossNetworksPrefersHomeThenAway() {
        assertEquals(HomeProbe.REACHED, listOf(HomeProbe.RESOLVED, HomeProbe.REACHED).best())
        assertEquals(HomeProbe.RESOLVED, listOf(HomeProbe.UNRESOLVED, HomeProbe.RESOLVED).best())
        assertEquals(HomeProbe.UNRESOLVED, listOf(HomeProbe.UNRESOLVED).best())
    }
}

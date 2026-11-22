package dev.proflyder.home.feature.vpn.environment

import java.net.InetAddress
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class EnvironmentRulesTest {
    @Test
    fun homeAddressesArePrivateOnly() {
        assertTrue(HomeAddress.isPrivate(InetAddress.getByName("192.168.1.10")))
        assertTrue(HomeAddress.isPrivate(InetAddress.getByName("10.0.0.2")))
        assertTrue(HomeAddress.isPrivate(InetAddress.getByName("fd00::1")))
        assertFalse(HomeAddress.isPrivate(InetAddress.getByName("203.0.113.5")))
        assertFalse(HomeAddress.isPrivate(InetAddress.getByName("2001:db8::1")))
    }

    @Test
    fun settleLetsTheFirstChangeThroughAndCollapsesAFlappingBurst() = runTest {
        val changes = flow {
            emit("wifi")
            delay(100)
            emit("mobile")
            delay(100)
            emit("wifi-2")
            delay(2_000)
            emit("home")
        }
        assertEquals(listOf("wifi", "wifi-2", "home"), changes.settle(1_500.milliseconds).toList())
    }
}

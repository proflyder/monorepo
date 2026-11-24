package dev.proflyder.home.feature.vpn.environment

import dev.proflyder.home.feature.vpn.HomeNetworkState
import org.junit.Assert.assertEquals
import org.junit.Test

class EnvironmentClassifierTest {
    private val wifi = NetworkFacts(setOf(1L))

    @Test
    fun aBlockedNetworkIsUnknownAndIsNotProbed() {
        val state = EnvironmentClassifier.classify(wifi.copy(blocked = setOf(1L)), endpointKnown = true) { error("probed") }
        assertEquals(HomeNetworkState.Unknown, state)
    }

    @Test
    fun withoutAServerEndpointTheNetworkIsUnknownNotAway() {
        assertEquals(HomeNetworkState.Unknown, EnvironmentClassifier.classify(wifi, endpointKnown = false) { false })
    }

    @Test
    fun reachableHomeIsHomeOtherwiseAwayAndNoNetworkIsOffline() {
        assertEquals(HomeNetworkState.Home(setOf(1L)), EnvironmentClassifier.classify(wifi, true) { true })
        assertEquals(HomeNetworkState.Away(setOf(1L)), EnvironmentClassifier.classify(wifi, true) { false })
        assertEquals(HomeNetworkState.Offline, EnvironmentClassifier.classify(NetworkFacts(emptySet()), true) { true })
    }

    @Test
    fun aBlockedHomeNetworkKeepsTheKnownHomeInsteadOfFlappingToUnknown() {
        val home = HomeNetworkState.Home(setOf(1L))

        assertEquals(home, EnvironmentClassifier.keepKnownHome(HomeNetworkState.Unknown, home, setOf(1L)))
    }

    @Test
    fun aBlockedNetworkOnDifferentHandlesIsUnknown() {
        val home = HomeNetworkState.Home(setOf(1L))

        assertEquals(HomeNetworkState.Unknown, EnvironmentClassifier.keepKnownHome(HomeNetworkState.Unknown, home, setOf(2L)))
    }

    @Test
    fun losingEveryNetworkIsOfflineEvenAfterHome() {
        val home = HomeNetworkState.Home(setOf(1L))

        assertEquals(HomeNetworkState.Offline, EnvironmentClassifier.keepKnownHome(HomeNetworkState.Offline, home, setOf(1L)))
    }
}

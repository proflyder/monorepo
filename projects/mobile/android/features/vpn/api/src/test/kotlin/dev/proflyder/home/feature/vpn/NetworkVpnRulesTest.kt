package dev.proflyder.home.feature.vpn

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class NetworkVpnRulesTest {
    private val away = NetworkVpnInput(
        connectOnOpen = true,
        autoConnect = true,
        atHome = false,
        active = false,
        endpointStale = false,
        hasProfile = true,
        permissionGranted = true,
        permissionAsked = false,
    )

    private val homeWifi = NetworkState(atHome = true, networks = setOf(WIFI))
    private val foreignWifi = NetworkState(atHome = false, networks = setOf(OTHER_WIFI))
    private val mobile = NetworkState(atHome = false, networks = setOf(CELLULAR))

    @Test
    fun connectsAwayFromHome() = assertEquals(NetworkVpnAction.CONNECT, NetworkVpnRules.decide(away))

    @Test
    fun staysOffAtHome() =
        assertEquals(NetworkVpnAction.NONE, NetworkVpnRules.decide(away.copy(atHome = true)))

    @Test
    fun staysOffWhenNetworkIsUnknown() =
        assertEquals(NetworkVpnAction.NONE, NetworkVpnRules.decide(away.copy(atHome = null, active = true)))

    @Test
    fun staysOffWhenBothSettingsAreOff() = assertEquals(
        NetworkVpnAction.NONE,
        NetworkVpnRules.decide(away.copy(connectOnOpen = false, autoConnect = false)),
    )

    @Test
    fun keepsRunningTunnel() =
        assertEquals(NetworkVpnAction.NONE, NetworkVpnRules.decide(away.copy(active = true)))

    @Test
    fun needsProfile() =
        assertEquals(NetworkVpnAction.NONE, NetworkVpnRules.decide(away.copy(hasProfile = false)))

    @Test
    fun asksPermissionOnce() {
        val missing = away.copy(permissionGranted = false)
        assertEquals(NetworkVpnAction.ASK_PERMISSION, NetworkVpnRules.decide(missing))
        assertEquals(NetworkVpnAction.NONE, NetworkVpnRules.decide(missing.copy(permissionAsked = true)))
    }

    @Test
    fun reconnectsWithStaleEndpoint() = assertEquals(
        NetworkVpnAction.RECONNECT,
        NetworkVpnRules.decide(away.copy(active = true, endpointStale = true)),
    )

    @Test
    fun disconnectsBackHomeOnlyWithAwaySetting() {
        val home = away.copy(atHome = true, active = true, endpointStale = true)
        assertEquals(NetworkVpnAction.DISCONNECT, NetworkVpnRules.decide(home))
        assertEquals(NetworkVpnAction.RECONNECT, NetworkVpnRules.decide(home.copy(autoConnect = false)))
    }

    @Test
    fun followsHomeMobileHomeSequence() = runTest {
        val tunnel = FakeTunnel()
        val actions = tunnel.run(timed(0L to homeWifi, 5_000L to mobile, 5_000L to homeWifi)).toList()
        assertEquals(listOf(NetworkVpnAction.NONE, NetworkVpnAction.CONNECT, NetworkVpnAction.DISCONNECT), actions)
    }

    @Test
    fun followsHomeForeignWifiMobileHomeSequence() = runTest {
        val tunnel = FakeTunnel()
        val steps = timed(0L to homeWifi, 5_000L to foreignWifi, 5_000L to mobile, 5_000L to homeWifi)
        val expected = listOf(
            NetworkVpnAction.NONE,
            NetworkVpnAction.CONNECT,
            NetworkVpnAction.NONE,
            NetworkVpnAction.DISCONNECT,
        )
        assertEquals(expected, tunnel.run(steps).toList())
    }

    @Test
    fun reconnectsOnNetworkChangeWhenEndpointIsStale() = runTest {
        val tunnel = FakeTunnel(autoConnect = false)
        val steps = timed(0L to foreignWifi, 5_000L to homeWifi, 5_000L to mobile)
        val expected = listOf(NetworkVpnAction.CONNECT, NetworkVpnAction.RECONNECT, NetworkVpnAction.RECONNECT)
        assertEquals(expected, tunnel.run(steps).toList())
    }

    @Test
    fun debouncesQuickSwitching() = runTest {
        val tunnel = FakeTunnel()
        val flapping = timed(0L to homeWifi, 3_000L to mobile, 300L to homeWifi, 300L to mobile, 300L to homeWifi)
        assertEquals(listOf(NetworkVpnAction.NONE), tunnel.run(flapping).toList())
    }

    @Test
    fun settlesOnLastNetworkAfterSwitching() = runTest {
        val tunnel = FakeTunnel()
        val switching = timed(0L to homeWifi, 3_000L to mobile, 400L to homeWifi, 400L to mobile)
        assertEquals(listOf(NetworkVpnAction.NONE, NetworkVpnAction.CONNECT), tunnel.run(switching).toList())
    }

    @Test
    fun ignoresRepeatedSameNetwork() = runTest {
        val tunnel = FakeTunnel()
        val repeated = timed(0L to mobile, 5_000L to mobile, 5_000L to mobile)
        assertEquals(listOf(NetworkVpnAction.CONNECT), tunnel.run(repeated).toList())
    }

    private fun timed(vararg steps: Pair<Long, NetworkState>): Flow<NetworkState> = flow {
        steps.forEach { (pause, state) -> delay(pause).also { emit(state) } }
    }

    private inner class FakeTunnel(private val autoConnect: Boolean = true) {
        var active = false
        var endpointAtHome: Boolean? = null

        fun run(changes: Flow<NetworkState>): Flow<NetworkVpnAction> =
            VpnNetworkFlow.decisions(VpnNetworkFlow.settle(changes), ::input).onEach(::apply)

        private var current: NetworkState? = null

        private fun input(state: NetworkState): NetworkVpnInput {
            current = state
            val stale = active && endpointAtHome != state.atHome
            return away.copy(autoConnect = autoConnect, atHome = state.atHome, active = active, endpointStale = stale)
        }

        private fun apply(action: NetworkVpnAction) {
            active = when (action) {
                NetworkVpnAction.CONNECT, NetworkVpnAction.RECONNECT -> true
                NetworkVpnAction.DISCONNECT -> false
                else -> active
            }
            endpointAtHome = current?.atHome?.takeIf { active }
        }
    }

    private companion object {
        const val WIFI = 100L
        const val CELLULAR = 200L
        const val OTHER_WIFI = 300L
    }
}

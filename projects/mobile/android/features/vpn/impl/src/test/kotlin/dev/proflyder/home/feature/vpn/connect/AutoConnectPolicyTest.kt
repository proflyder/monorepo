package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.feature.vpn.HomeNetworkState
import org.junit.Assert.assertEquals
import org.junit.Test

class AutoConnectPolicyTest {
    private val away = AutoConnectInput(
        foreground = true,
        autoConnect = false,
        connectOnOpen = true,
        environment = HomeNetworkState.Away(setOf(1L)),
        active = false,
        hasProfile = true,
        permissionGranted = true,
        permissionAsked = false,
    )

    @Test
    fun connectsAwayFromHomeWhenASettingAsksForIt() {
        assertEquals(AutoConnectAction.CONNECT, AutoConnectPolicy.decide(away))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(away.copy(connectOnOpen = false)))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(away.copy(foreground = false)))
        assertEquals(AutoConnectAction.CONNECT, AutoConnectPolicy.decide(away.copy(foreground = false, autoConnect = true)))
    }

    @Test
    fun unknownNetworkActiveTunnelOrNoProfileDoNothing() {
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(away.copy(environment = HomeNetworkState.Unknown)))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(away.copy(environment = HomeNetworkState.Offline)))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(away.copy(active = true)))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(away.copy(hasProfile = false)))
    }

    @Test
    fun permissionIsAskedOnceAndOnlyInTheForeground() {
        val missing = away.copy(permissionGranted = false)
        assertEquals(AutoConnectAction.ASK_PERMISSION, AutoConnectPolicy.decide(missing))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(missing.copy(permissionAsked = true)))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(missing.copy(foreground = false, autoConnect = true)))
    }

    @Test
    fun homeDisconnectsOnlyWithAutoConnect() {
        val home = away.copy(environment = HomeNetworkState.Home(setOf(1L)), active = true, autoConnect = true)
        assertEquals(AutoConnectAction.DISCONNECT, AutoConnectPolicy.decide(home))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(home.copy(autoConnect = false)))
        assertEquals(AutoConnectAction.NONE, AutoConnectPolicy.decide(home.copy(active = false)))
    }
}

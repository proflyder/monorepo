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

    private fun decide(input: AutoConnectInput): AutoConnectAction = AutoConnectPolicy.decide(input).action

    @Test
    fun homeNeverConnectsWhateverTheSettingsAndForeground() {
        val home = away.copy(environment = HomeNetworkState.Home(setOf(1L)), autoConnect = true)
        assertEquals(AutoConnectAction.NONE, decide(home))
        assertEquals(AutoConnectAction.NONE, decide(home.copy(foreground = false)))
        assertEquals(AutoConnectReason.AT_HOME, AutoConnectPolicy.decide(home).reason)
    }

    @Test
    fun unknownEnvironmentNeverConnectsEvenWithAutoConnect() {
        val unknown = away.copy(environment = HomeNetworkState.Unknown, autoConnect = true)
        assertEquals(AutoConnectAction.NONE, decide(unknown))
        assertEquals(AutoConnectAction.NONE, decide(unknown.copy(foreground = false)))
        assertEquals(AutoConnectReason.ENVIRONMENT_UNKNOWN, AutoConnectPolicy.decide(unknown).reason)
    }

    @Test
    fun backgroundConnectsOnlyWithAutoConnectAway() {
        val background = away.copy(foreground = false, connectOnOpen = true)
        assertEquals(AutoConnectDecision(AutoConnectAction.NONE, AutoConnectReason.NOT_REQUESTED), AutoConnectPolicy.decide(background))
        assertEquals(AutoConnectAction.CONNECT, decide(background.copy(autoConnect = true)))
        assertEquals(AutoConnectAction.NONE, decide(background.copy(autoConnect = true, userWish = TunnelWish.DOWN)))
    }

    @Test
    fun connectsAwayFromHomeWhenASettingAsksForIt() {
        assertEquals(AutoConnectAction.CONNECT, decide(away))
        assertEquals(AutoConnectAction.NONE, decide(away.copy(connectOnOpen = false)))
        assertEquals(AutoConnectAction.NONE, decide(away.copy(foreground = false)))
        assertEquals(AutoConnectAction.CONNECT, decide(away.copy(foreground = false, autoConnect = true)))
    }

    @Test
    fun unknownNetworkActiveTunnelOrNoProfileDoNothing() {
        assertEquals(AutoConnectAction.NONE, decide(away.copy(environment = HomeNetworkState.Unknown)))
        assertEquals(AutoConnectAction.NONE, decide(away.copy(environment = HomeNetworkState.Offline)))
        assertEquals(AutoConnectAction.NONE, decide(away.copy(active = true)))
        assertEquals(AutoConnectAction.NONE, decide(away.copy(hasProfile = false)))
    }

    @Test
    fun permissionIsAskedOnceAndOnlyInTheForeground() {
        val missing = away.copy(permissionGranted = false)
        assertEquals(AutoConnectAction.ASK_PERMISSION, decide(missing))
        assertEquals(AutoConnectAction.NONE, decide(missing.copy(permissionAsked = true)))
        assertEquals(AutoConnectAction.NONE, decide(missing.copy(foreground = false, autoConnect = true)))
    }

    @Test
    fun homeDisconnectsOnlyWithAutoConnect() {
        val home = away.copy(environment = HomeNetworkState.Home(setOf(1L)), active = true, autoConnect = true)
        assertEquals(AutoConnectAction.DISCONNECT, decide(home))
        assertEquals(AutoConnectAction.NONE, decide(home.copy(autoConnect = false)))
        assertEquals(AutoConnectAction.NONE, decide(home.copy(active = false)))
    }

    @Test
    fun aManualChoiceOverridesThePolicyInItsNetwork() {
        val home = away.copy(environment = HomeNetworkState.Home(setOf(1L)), autoConnect = true, active = true)
        assertEquals(AutoConnectAction.DISCONNECT, decide(home))
        assertEquals(AutoConnectAction.NONE, decide(home.copy(userWish = TunnelWish.UP)))
        val awayAuto = away.copy(autoConnect = true)
        assertEquals(AutoConnectAction.NONE, decide(awayAuto.copy(userWish = TunnelWish.DOWN)))
    }

    @Test
    fun aChoiceLastsOnlyInTheNetworkWhereItWasMade() {
        val choice = UserVpnChoice(TunnelWish.UP, setOf(1L))
        assertEquals(choice, choice.on(HomeNetworkState.Home(setOf(1L))))
        assertEquals(choice, choice.on(HomeNetworkState.Unknown))
        assertEquals(null, choice.on(HomeNetworkState.Away(setOf(2L))))
        assertEquals(null, choice.on(HomeNetworkState.Offline))
        val unbound = UserVpnChoice(TunnelWish.DOWN, null)
        assertEquals(unbound.copy(networks = setOf(3L)), unbound.on(HomeNetworkState.Away(setOf(3L))))
    }
}

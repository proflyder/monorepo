package dev.proflyder.home.feature.vpn

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OpenConnectRulesTest {
    private val away = OpenConnectInput(
        enabled = true,
        atHome = false,
        active = false,
        hasProfile = true,
        permissionGranted = true,
        permissionAsked = false,
    )

    @Test
    fun connectsAwayFromHomeWithProfileAndPermission() =
        assertEquals(OpenConnectAction.CONNECT, OpenConnectRules.decide(away))

    @Test
    fun staysOffAtHome() =
        assertEquals(OpenConnectAction.NONE, OpenConnectRules.decide(away.copy(atHome = true)))

    @Test
    fun staysOffWhenHomeIsUnknown() =
        assertEquals(OpenConnectAction.NONE, OpenConnectRules.decide(away.copy(atHome = null)))

    @Test
    fun staysOffWithoutProfile() =
        assertEquals(OpenConnectAction.NONE, OpenConnectRules.decide(away.copy(hasProfile = false)))

    @Test
    fun staysOffWhenTunnelIsAlreadyUp() =
        assertEquals(OpenConnectAction.NONE, OpenConnectRules.decide(away.copy(active = true)))

    @Test
    fun staysOffWhenSettingIsDisabled() =
        assertEquals(OpenConnectAction.NONE, OpenConnectRules.decide(away.copy(enabled = false)))

    @Test
    fun asksForPermissionOnce() {
        val missing = away.copy(permissionGranted = false)
        assertEquals(OpenConnectAction.ASK_PERMISSION, OpenConnectRules.decide(missing))
        assertEquals(OpenConnectAction.NONE, OpenConnectRules.decide(missing.copy(permissionAsked = true)))
    }

    @Test
    fun doesNotAskForPermissionAtHome() {
        val missing = away.copy(permissionGranted = false, atHome = true)
        assertEquals(OpenConnectAction.NONE, OpenConnectRules.decide(missing))
    }

    @Test
    fun grantedPermissionConnectsEvenAfterAsking() =
        assertEquals(OpenConnectAction.CONNECT, OpenConnectRules.decide(away.copy(permissionAsked = true)))

    @Test
    fun onlyConnectingIsUnsettled() {
        assertFalse(OpenConnectRules.settled(VpnConnection.Connecting(VpnProtocol.WIREGUARD)))
        assertTrue(OpenConnectRules.settled(VpnConnection.Connected(VpnProtocol.WIREGUARD, 1)))
        assertTrue(OpenConnectRules.settled(VpnConnection.FallbackRequired))
        assertTrue(OpenConnectRules.settled(VpnConnection.NotNeededAtHome))
        assertTrue(OpenConnectRules.settled(VpnConnection.Failed(VpnConnectProblem.TUNNEL)))
    }

    @Test
    fun connectOnOpenIsOnByDefault() {
        assertTrue(VpnSettings().connectOnOpen)
        assertFalse(VpnSettings().permissionAsked)
    }
}

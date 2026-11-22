package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.feature.vpn.presentation.VpnSamples
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class StoredVpnProfileTest {
    @Test
    fun profileSurvivesTheStoredRoundTrip() {
        val profile = VpnSamples.profile
        assertEquals(profile, StoredVpnProfile.decode(StoredVpnProfile.encode(profile)))
    }

    @Test
    fun profileStoredBeforeTheLanEndpointStillLoads() {
        val legacy = StoredVpnProfile.encode(VpnSamples.profile).replace(",\"lanEndpoint\":\"192.0.2.29:51820\"", "")
        assertFalse(legacy.contains("lanEndpoint"))
        assertEquals("", StoredVpnProfile.decode(legacy)?.wireguard?.lanEndpoint)
    }

    @Test
    fun brokenStoredTextIsIgnored() {
        assertNull(StoredVpnProfile.decode("{not json"))
        assertNull(StoredVpnProfile.decode(null))
    }
}

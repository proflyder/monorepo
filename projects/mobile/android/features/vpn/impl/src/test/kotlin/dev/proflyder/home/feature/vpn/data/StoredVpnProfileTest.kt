package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.feature.vpn.presentation.VpnSamples
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StoredVpnProfileTest {
    @Test
    fun profileSurvivesTheStoredRoundTrip() {
        val profile = VpnSamples.profile
        assertEquals(profile, StoredVpnProfile.decode(StoredVpnProfile.encode(profile)))
    }

    @Test
    fun brokenStoredTextIsIgnored() {
        assertNull(StoredVpnProfile.decode("{not json"))
        assertNull(StoredVpnProfile.decode(null))
    }
}

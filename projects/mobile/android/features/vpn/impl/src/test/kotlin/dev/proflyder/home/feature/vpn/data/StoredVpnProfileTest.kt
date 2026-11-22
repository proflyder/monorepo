package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.vpn.VpnProfileProblem
import dev.proflyder.home.feature.vpn.VpnSamples
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StoredVpnProfileTest {
    @Test
    fun roundTripKeepsEveryTransportUntouched() {
        assertEquals(VpnSamples.profile, StoredVpnProfile.decode(StoredVpnProfile.encode(VpnSamples.profile)))
    }

    @Test
    fun brokenOrMissingTextIsNoProfile() {
        assertNull(StoredVpnProfile.decode("{not json"))
        assertNull(StoredVpnProfile.decode(null))
    }

    @Test
    fun serverCodesMapToProblems() {
        assertEquals(VpnProfileProblem.CONFLICT, VpnProblems.of(RemoteFailure.Server(409, ConnectCode.UNKNOWN, "")))
        assertEquals(VpnProfileProblem.LIMIT, VpnProblems.of(RemoteFailure.Server(429, ConnectCode.UNKNOWN, "")))
        assertEquals(VpnProfileProblem.UNAVAILABLE, VpnProblems.of(RemoteFailure.Server(503, ConnectCode.UNKNOWN, "")))
        assertEquals(VpnProfileProblem.INVALID, VpnProblems.of(RemoteFailure.Server(400, ConnectCode.UNKNOWN, "")))
        assertEquals(VpnProfileProblem.OFFLINE, VpnProblems.of(RemoteFailure.Timeout))
        assertEquals(VpnProfileProblem.UNAUTHORIZED, VpnProblems.of(RemoteFailure.Unauthorized))
    }
}

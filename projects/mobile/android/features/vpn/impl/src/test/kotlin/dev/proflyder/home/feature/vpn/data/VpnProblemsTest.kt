package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.vpn.VpnProfileProblem
import org.junit.Assert.assertEquals
import org.junit.Test

class VpnProblemsTest {
    @Test
    fun aModuleForbiddenBecomesNoAccessNotAGenericServerError() {
        assertEquals(VpnProfileProblem.NO_ACCESS, VpnProblems.of(RemoteFailure.ModuleForbidden("vpn")))
        assertEquals(VpnProfileProblem.NO_ACCESS, VpnProblems.of(RemoteFailure.ModuleForbidden("")))
    }

    @Test
    fun unauthorizedAndHttpProblemsKeepTheirMeaning() {
        assertEquals(VpnProfileProblem.UNAUTHORIZED, VpnProblems.of(RemoteFailure.Unauthorized))
        assertEquals(
            VpnProfileProblem.LIMIT,
            VpnProblems.of(RemoteFailure.Server(429, dev.proflyder.home.core.model.ConnectCode.UNKNOWN, "limit")),
        )
    }
}

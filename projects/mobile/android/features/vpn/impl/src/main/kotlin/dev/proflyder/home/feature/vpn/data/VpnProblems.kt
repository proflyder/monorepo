package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.vpn.VpnProfileProblem

internal object VpnProblems {
    fun of(failure: RemoteFailure): VpnProfileProblem = when (failure) {
        RemoteFailure.NotConfigured -> VpnProfileProblem.NOT_CONFIGURED
        RemoteFailure.Unauthorized -> VpnProfileProblem.UNAUTHORIZED
        is RemoteFailure.ModuleForbidden -> VpnProfileProblem.NO_ACCESS
        is RemoteFailure.Offline, RemoteFailure.Timeout, is RemoteFailure.Tls -> VpnProfileProblem.OFFLINE
        is RemoteFailure.Server -> ofHttpStatus(failure.httpStatus)
        else -> VpnProfileProblem.SERVER
    }

    private fun ofHttpStatus(status: Int): VpnProfileProblem = when (status) {
        HTTP_BAD_REQUEST -> VpnProfileProblem.INVALID
        HTTP_CONFLICT -> VpnProfileProblem.CONFLICT
        HTTP_TOO_MANY_REQUESTS -> VpnProfileProblem.LIMIT
        HTTP_UNAVAILABLE -> VpnProfileProblem.UNAVAILABLE
        else -> VpnProfileProblem.SERVER
    }

    private const val HTTP_BAD_REQUEST = 400
    private const val HTTP_CONFLICT = 409
    private const val HTTP_TOO_MANY_REQUESTS = 429
    private const val HTTP_UNAVAILABLE = 503
}

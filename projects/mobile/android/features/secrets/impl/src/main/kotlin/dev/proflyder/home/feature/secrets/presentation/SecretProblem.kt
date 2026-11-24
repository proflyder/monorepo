package dev.proflyder.home.feature.secrets.presentation

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.RemoteFailure

internal sealed interface SecretProblem {
    data object StepUpRequired : SecretProblem

    data object InvalidSignature : SecretProblem

    data object DeviceNotEnrolled : SecretProblem

    data object StepUpExpired : SecretProblem

    data object DeviceUnavailable : SecretProblem

    data object DeviceRebindRequired : SecretProblem

    data object EnrollmentRejected : SecretProblem

    data object EnrollmentConflict : SecretProblem

    data object VersionUnavailable : SecretProblem

    data class Conflict(val currentVersion: Long?) : SecretProblem

    data class RateLimited(val retryAfterSeconds: Long?) : SecretProblem

    data object Forbidden : SecretProblem

    data object KeyMissing : SecretProblem

    data object NotFound : SecretProblem

    data class Other(val failure: RemoteFailure) : SecretProblem
}

internal fun secretProblem(failure: RemoteFailure): SecretProblem {
    val server = failure as? RemoteFailure.Server ?: return SecretProblem.Other(failure)
    val currentVersion = server.textArgs["currentVersion"]?.toLongOrNull()
    val retryAfter = server.textArgs["retryAfter"]?.toLongOrNull()
    return when {
        server.reason == "step_up_required" -> SecretProblem.StepUpRequired
        server.reason == "step_up_invalid" -> SecretProblem.InvalidSignature
        server.reason == "step_up_expired" -> SecretProblem.StepUpExpired
        server.reason == "device_not_enrolled" -> SecretProblem.DeviceNotEnrolled
        server.reason == "device_unavailable" -> SecretProblem.DeviceUnavailable
        server.reason == "enrollment_invalid" ||
            server.reason == "enrollment_closed" ||
            server.reason == "enrollment_expired" ||
            server.reason == "public_key_invalid" ||
            server.reason == "attestation_invalid" ||
            server.reason == "attestation_challenge_invalid" -> SecretProblem.EnrollmentRejected
        server.reason == "enrollment_conflict" -> SecretProblem.EnrollmentConflict
        server.reason == "version_unavailable" -> SecretProblem.VersionUnavailable
        server.reason == "version_not_current" -> SecretProblem.Conflict(currentVersion)
        server.reason == "conflict" || server.httpStatus == 409 -> SecretProblem.Conflict(currentVersion)
        server.reason == "rate_limited" || server.httpStatus == 429 -> SecretProblem.RateLimited(retryAfter)
        server.reason == "path_forbidden" -> SecretProblem.Forbidden
        server.reason == "key_not_found" -> SecretProblem.KeyMissing
        server.reason == "version_not_found" || server.reason == "not_found" -> SecretProblem.NotFound
        server.httpStatus == 403 -> SecretProblem.Forbidden
        server.code == ConnectCode.PERMISSION_DENIED -> SecretProblem.Forbidden
        else -> SecretProblem.Other(failure)
    }
}

internal fun signatureProblem(reason: DeviceSignatureFailure): SecretProblem = when (reason) {
    DeviceSignatureFailure.KEY_MISSING -> SecretProblem.DeviceNotEnrolled
    DeviceSignatureFailure.KEY_INVALIDATED -> SecretProblem.DeviceRebindRequired
    DeviceSignatureFailure.UNSUPPORTED,
    DeviceSignatureFailure.KEYSTORE,
    DeviceSignatureFailure.AUTHENTICATION,
    -> SecretProblem.DeviceUnavailable
}

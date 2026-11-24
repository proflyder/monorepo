package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.secrets.presentation.DeviceSignatureFailure
import dev.proflyder.home.feature.secrets.presentation.SecretProblem
import dev.proflyder.home.feature.secrets.presentation.secretProblem
import dev.proflyder.home.feature.secrets.presentation.signatureProblem
import org.junit.Assert.assertEquals
import org.junit.Test

class SecretProblemTest {
    private fun server(reason: String, status: Int = 400, args: Map<String, String> = emptyMap()) =
        RemoteFailure.Server(status, ConnectCode.UNKNOWN, reason, reason, textArgs = args)

    @Test
    fun deviceStepUpCodesMapToTheirOwnProblems() {
        assertEquals(SecretProblem.StepUpRequired, secretProblem(server("step_up_required", 403)))
        assertEquals(SecretProblem.InvalidSignature, secretProblem(server("step_up_invalid", 403)))
        assertEquals(SecretProblem.StepUpExpired, secretProblem(server("step_up_expired", 403)))
        assertEquals(SecretProblem.DeviceNotEnrolled, secretProblem(server("device_not_enrolled", 403)))
        assertEquals(SecretProblem.DeviceUnavailable, secretProblem(server("device_unavailable", 503)))
    }

    @Test
    fun enrollmentFailuresMapToTheEnrollmentProblems() {
        assertEquals(SecretProblem.EnrollmentRejected, secretProblem(server("enrollment_invalid", 403)))
        assertEquals(SecretProblem.EnrollmentRejected, secretProblem(server("enrollment_expired", 403)))
        assertEquals(SecretProblem.EnrollmentConflict, secretProblem(server("enrollment_conflict", 409)))
    }

    @Test
    fun aConflictCarriesTheCurrentVersion() {
        assertEquals(
            SecretProblem.Conflict(7),
            secretProblem(server("conflict", 409, mapOf("currentVersion" to "7"))),
        )
    }

    @Test
    fun aDeletedOrDestroyedVersionIsUnavailable() {
        assertEquals(SecretProblem.VersionUnavailable, secretProblem(server("version_unavailable", 409)))
    }

    @Test
    fun rateLimitingCarriesTheRetryAfterSeconds() {
        assertEquals(
            SecretProblem.RateLimited(120),
            secretProblem(server("rate_limited", 429, mapOf("retryAfter" to "120"))),
        )
    }

    @Test
    fun forbiddenPathsAndKeysAreSeparate() {
        assertEquals(SecretProblem.Forbidden, secretProblem(server("path_forbidden", 403)))
        assertEquals(SecretProblem.KeyMissing, secretProblem(server("key_not_found", 404)))
        assertEquals(SecretProblem.NotFound, secretProblem(server("not_found", 404)))
    }

    @Test
    fun localSignerFailuresMapToDeviceProblems() {
        assertEquals(SecretProblem.DeviceNotEnrolled, signatureProblem(DeviceSignatureFailure.KEY_MISSING))
        assertEquals(SecretProblem.DeviceRebindRequired, signatureProblem(DeviceSignatureFailure.KEY_INVALIDATED))
        assertEquals(SecretProblem.DeviceUnavailable, signatureProblem(DeviceSignatureFailure.UNSUPPORTED))
        assertEquals(SecretProblem.DeviceUnavailable, signatureProblem(DeviceSignatureFailure.KEYSTORE))
        assertEquals(SecretProblem.DeviceUnavailable, signatureProblem(DeviceSignatureFailure.AUTHENTICATION))
    }

    @Test
    fun unauthorizedFailureStaysGeneric() {
        assertEquals(SecretProblem.Other(RemoteFailure.Unauthorized), secretProblem(RemoteFailure.Unauthorized))
    }
}

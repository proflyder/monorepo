package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.secrets.presentation.SecretProblem
import dev.proflyder.home.feature.secrets.presentation.secretProblem
import org.junit.Assert.assertEquals
import org.junit.Test

class SecretProblemTest {
    private fun server(reason: String, status: Int = 400, args: Map<String, String> = emptyMap()) =
        RemoteFailure.Server(status, ConnectCode.UNKNOWN, reason, reason, textArgs = args)

    @Test
    fun stepUpAndTotpCodesMapToTheirOwnProblems() {
        assertEquals(SecretProblem.StepUpRequired, secretProblem(server("step_up_required", 403)))
        assertEquals(SecretProblem.InvalidTotp, secretProblem(server("step_up_invalid", 403)))
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
    fun unauthorizedFailureStaysGeneric() {
        assertEquals(SecretProblem.Other(RemoteFailure.Unauthorized), secretProblem(RemoteFailure.Unauthorized))
    }
}

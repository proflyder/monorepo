package dev.proflyder.home.core.config

import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ConnectionConfigTest {
    private val endpoint = (ServerEndpoint.parse("http://192.0.2.10:8080") as EndpointParseResult.Valid).endpoint

    @Test
    fun toStringRedactsBearerToken() {
        val config = ConnectionConfig(endpoint = endpoint, bearerToken = "super-secret-token")
        val text = config.toString()
        assertFalse(text.contains("super-secret-token"))
        assertTrue(text.contains("<set>"))
        assertTrue(text.contains("http://192.0.2.10:8080"))
    }

    @Test
    fun validatesBearerTokenHeaderSafety() {
        fun field(token: String?): Field? {
            val result = ConnectionConfig(endpoint = endpoint, bearerToken = token).validated()
            return (result as? Outcome.Failure)?.failure
                ?.let { (it as? RemoteFailure.Validation)?.field }
        }

        assertEquals(Field.TOKEN, field(null))
        assertEquals(Field.TOKEN, field(""))
        assertEquals(Field.TOKEN, field("   "))
        assertEquals(Field.TOKEN, field("bad\r\nX-Evil: 1"))
        assertEquals(Field.TOKEN, field("bad\u0000token"))
        assertEquals(Field.TOKEN, field("token with space"))
        assertEquals(Field.TOKEN, field("tökén"))
        assertEquals(Field.TOKEN, field("x".repeat(ConnectionConfig.MAX_TOKEN_LENGTH + 1)))
        assertTrue(ConnectionConfig(endpoint = endpoint, bearerToken = "token-ok_123").validated() is Outcome.Success)
    }

    @Test
    fun clearedTokenIsStorageValidButNotRequestReady() {
        val cleared = ConnectionConfig(endpoint = endpoint, bearerToken = null)
        assertTrue(cleared.validatedForStorage() is Outcome.Success)
        assertEquals(
            Field.TOKEN,
            ((cleared.validated() as Outcome.Failure).failure as RemoteFailure.Validation).field,
        )

        val unsafe = ConnectionConfig(endpoint = endpoint, bearerToken = "bad\ntoken")
        assertEquals(
            Field.TOKEN,
            ((unsafe.validatedForStorage() as Outcome.Failure).failure as RemoteFailure.Validation).field,
        )
    }

    @Test
    fun unsetConfigIsReportedHonestly() {
        val config = ConnectionConfig()
        assertFalse(config.isConfigured)
        assertFalse(config.hasToken)
        assertTrue(config.toString().contains("<unset>"))
    }

    @Test
    fun validatesEndpointAndTimeoutBoundaries() {
        assertTrue(
            ConnectionConfig(endpoint = endpoint, bearerToken = "valid-token", connectTimeoutMillis = 1_000, readTimeoutMillis = 1_000)
                .validated() is Outcome.Success,
        )
        assertTrue(
            ConnectionConfig(endpoint = endpoint, bearerToken = "valid-token", connectTimeoutMillis = 120_000, readTimeoutMillis = 120_000)
                .validated() is Outcome.Success,
        )
        val noEndpoint = ConnectionConfig().validated()
        assertEquals(
            Field.ENDPOINT,
            (noEndpoint as Outcome.Failure).failure.let { (it as RemoteFailure.Validation).field },
        )
        val badTimeout = ConnectionConfig(endpoint = endpoint, bearerToken = "valid-token", connectTimeoutMillis = 0).validated()
        assertEquals(
            Field.TIMEOUT,
            (badTimeout as Outcome.Failure).failure.let { (it as RemoteFailure.Validation).field },
        )
    }
}

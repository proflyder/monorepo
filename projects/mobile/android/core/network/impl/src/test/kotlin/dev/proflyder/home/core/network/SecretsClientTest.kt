package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.testing.TestFixtures
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
import okhttp3.Interceptor
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class SecretsClientTest {
    private lateinit var server: MockWebServer

    @Before
    fun setUp() {
        server = MockWebServer().apply { start() }
    }

    @After
    fun tearDown() = server.shutdown()

    private class Store(config: ConnectionConfig) : ConnectionSettingsStore {
        override val config: StateFlow<ConnectionConfig> = MutableStateFlow(config)
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.AVAILABLE)
        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }

    private fun client(interceptors: Set<Interceptor> = emptySet()): HttpSecretsClient {
        val endpoint = (ServerEndpoint.parse(server.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint
        val http = testHttp(
            Store(ConnectionConfig(endpoint = endpoint, secretsEndpoint = endpoint, bearerToken = "tok-1")),
            interceptors = interceptors,
        )
        return HttpSecretsClient(http)
    }

    private fun respond(code: Int, fixture: String) {
        server.enqueue(MockResponse().setResponseCode(code).setBody(TestFixtures.read("secrets/$fixture")))
    }

    @Test
    fun theCatalogCarriesDescriptionsConsumersAndTheTruncatedFlag() = runBlocking {
        respond(200, "catalog.json")

        val catalog = (client().catalog() as Outcome.Success).value

        assertEquals(listOf("ci/android-distribute", "k8s/home-assistant/phone-push", "infra/legacy"), catalog.entries.map { it.path })
        assertEquals("CI android-app (OIDC)", catalog.entries.first().consumer)
        assertTrue(catalog.entries.first().inCatalog)
        assertFalse(catalog.entries.last().live)
        assertTrue(catalog.truncated)
        assertEquals("/v1/secrets", server.takeRequest().path)
    }

    @Test
    fun metadataExposesVersionsAndKeyNamesWithoutValues() = runBlocking {
        respond(200, "meta.json")

        val meta = (client().metadata("k8s/home-assistant/phone-push") as Outcome.Success).value

        assertEquals(3, meta.currentVersion)
        assertEquals(1, meta.oldestVersion)
        assertEquals(listOf(1L, 2L, 3L), meta.versions.map { it.version })
        assertFalse(meta.version(1)!!.available)
        assertFalse(meta.version(2)!!.available)
        assertTrue(meta.version(3)!!.available)
        assertEquals(listOf("endpoint", "service_token"), meta.keys)
        val request = server.takeRequest()
        assertEquals("/v1/secrets/meta/k8s/home-assistant/phone-push", request.path)
        assertFalse(request.body.readUtf8().contains("service-token-value"))
    }

    @Test
    fun revealSendsTheSessionHeaderAndReturnsTheSingleValue() = runBlocking {
        respond(200, "reveal.json")

        val value = (
            client().reveal("k8s/home-assistant/phone-push", "service_token", 3, "session-abc")
                as Outcome.Success
            ).value

        assertEquals("service-token-value", value.value)
        val request = server.takeRequest()
        assertEquals("/v1/secrets/reveal/k8s/home-assistant/phone-push", request.path)
        assertEquals("session-abc", request.getHeader("X-Step-Up-Session"))
        val body = request.body.readUtf8()
        assertTrue(body.contains("\"key\":\"service_token\""))
        assertTrue(body.contains("\"version\":3"))
    }

    @Test
    fun stepUpReturnsTheSessionAndItsLifetime() = runBlocking {
        respond(200, "stepup.json")

        val session = (client().stepUp(" 123456 ") as Outcome.Success).value

        assertEquals("step-up-session-value", session.session)
        assertEquals(300, session.expiresInSeconds)
        assertEquals("/v1/step-up", server.takeRequest().path)
    }

    @Test
    fun updateSendsTheKeyValueCasAndTotp() = runBlocking {
        respond(200, "update.json")

        val result = (client().update("k8s/home-assistant/phone-push", "service_token", "new", 3, "654321") as Outcome.Success).value

        assertEquals(4, result.version)
        val request = server.takeRequest()
        assertEquals("654321", request.getHeader("X-Step-Up-Totp"))
        val body = request.body.readUtf8()
        assertTrue(body.contains("\"cas\":3"))
        assertTrue(body.contains("\"value\":\"new\""))
    }

    @Test
    fun rollbackSendsTheSourceAndCurrentVersions() = runBlocking {
        respond(200, "rollback.json")

        val result = (client().rollback("k8s/home-assistant/phone-push", 1, 3, "111111") as Outcome.Success).value

        assertEquals(1, result.fromVersion)
        assertEquals(4, result.version)
        val request = server.takeRequest()
        assertEquals("111111", request.getHeader("X-Step-Up-Totp"))
        val body = request.body.readUtf8()
        assertTrue(body.contains("\"version\":1"))
        assertTrue(body.contains("\"cas\":3"))
    }

    @Test
    fun deleteCarriesCasVersionAndTotp() = runBlocking {
        respond(200, "delete.json")

        assertEquals(Outcome.Success(Unit), client().delete("k8s/home-assistant/phone-push", 3, 3, "222222"))

        val request = server.takeRequest()
        assertEquals("DELETE", request.method)
        assertEquals("/v1/secrets/k8s/home-assistant/phone-push?cas=3&version=3", request.path)
        assertEquals("222222", request.getHeader("X-Step-Up-Totp"))
    }

    @Test
    fun aConflictKeepsTheCurrentVersion() = runBlocking {
        respond(409, "conflict.json")

        val failure = client().update("k8s/home-assistant/phone-push", "k", "v", 3, "1") as Outcome.Failure

        assertEquals("conflict", (failure.failure as RemoteFailure.Server).reason)
        assertEquals("4", (failure.failure as RemoteFailure.Server).textArgs["currentVersion"])
    }

    @Test
    fun aDeletedVersionIsAConflictWithItsCode() = runBlocking {
        respond(409, "version_unavailable.json")

        val failure = client().update("k8s/home-assistant/phone-push", "k", "v", 3, "1") as Outcome.Failure

        assertEquals("version_unavailable", (failure.failure as RemoteFailure.Server).reason)
    }

    @Test
    fun rateLimitingKeepsTheRetryAfterSecondsFromHeaderOrBody() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(429)
                .setHeader("Retry-After", "120")
                .setBody(TestFixtures.read("secrets/rate_limited.json")),
        )

        val failure = client().stepUp("123456") as Outcome.Failure
        val serverFailure = failure.failure as RemoteFailure.Server

        assertEquals(429, serverFailure.httpStatus)
        assertEquals("rate_limited", serverFailure.reason)
        assertEquals("120", serverFailure.textArgs["retryAfter"])
    }

    @Test
    fun stepUpCodesSurviveTheForbiddenStatus() = runBlocking {
        respond(403, "step_up_required.json")

        val required = client().reveal("k8s/test", "key", 3, "expired") as Outcome.Failure
        assertEquals("step_up_required", (required.failure as RemoteFailure.Server).reason)

        respond(403, "step_up_invalid.json")
        val invalid = client().update("k8s/test", "key", "value", 3, "000000") as Outcome.Failure
        assertEquals("step_up_invalid", (invalid.failure as RemoteFailure.Server).reason)

        respond(403, "path_forbidden.json")
        val forbidden = client().metadata("k8s/test") as Outcome.Failure
        assertEquals("path_forbidden", (forbidden.failure as RemoteFailure.Server).reason)
    }

    @Test
    fun aMissingModuleStillMapsToTheGatingFailure() = runBlocking {
        respond(403, "module_forbidden.json")

        val failure = client().catalog() as Outcome.Failure

        assertEquals(RemoteFailure.ModuleForbidden("secrets"), failure.failure)
    }

    @Test
    fun theSecretsClientIsExcludedFromNetworkInspection() = runBlocking {
        val marker = Interceptor { chain ->
            chain.proceed(chain.request().newBuilder().header("X-Marker", "inspection").build())
        }
        val secrets = client(interceptors = setOf(marker))

        respond(200, "catalog.json")
        secrets.catalog()
        assertEquals(null, server.takeRequest().getHeader("X-Marker"))

        respond(200, "meta.json")
        secrets.metadata("k8s/home-assistant/phone-push")
        assertEquals(null, server.takeRequest().getHeader("X-Marker"))
    }
}

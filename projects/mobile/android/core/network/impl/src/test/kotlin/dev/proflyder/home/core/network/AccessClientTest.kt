package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.testing.TestFixtures
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class AccessClientTest {
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

    private fun client(): HttpAccessClient {
        val endpoint = (ServerEndpoint.parse(server.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint
        return HttpAccessClient(testHttp(Store(ConnectionConfig(endpoint = endpoint, bearerToken = "tok-1"))))
    }

    private fun respond(code: Int, fixture: String) {
        server.enqueue(MockResponse().setResponseCode(code).setBody(TestFixtures.read("access/$fixture")))
    }

    @Test
    fun meCarriesTheNameModulesAndOwnerFlag() = runBlocking {
        respond(200, "me.json")

        val access = (client().me() as Outcome.Success).value

        assertEquals("Телефон", access.name)
        assertEquals(setOf(AppModule.BASE, AppModule.HOME, AppModule.ALARMS_TIMERS), access.modules)
        assertFalse(access.owner)
        assertEquals("/v1/me", server.takeRequest().path)
    }

    @Test
    fun theTokenListParsesOwnersModulesAndUnixTimes() = runBlocking {
        respond(200, "tokens.json")

        val tokens = (client().tokens() as Outcome.Success).value

        assertEquals(listOf("owner", "t-phone"), tokens.map { it.id })
        assertTrue(tokens.first().owner)
        assertTrue(tokens.first().modules.contains(AppModule.ADMIN))
        assertEquals(setOf(AppModule.BASE, AppModule.HOME, AppModule.ALARMS_TIMERS), tokens.last().modules)
        assertEquals(1_700_000_300L, tokens.last().updatedUnix)
        assertEquals("/v1/access/tokens", server.takeRequest().path)
    }

    @Test
    fun createSendsTheNameAndGrantedModulesAndReturnsThePlainTokenOnce() = runBlocking {
        respond(201, "created.json")

        val created = (client().createToken(" Планшет ", setOf(AppModule.HOME, AppModule.NOTIFICATIONS)) as Outcome.Success).value

        assertEquals("home_0123456789abcdef0123456789abcdef", created.token)
        assertEquals("t-new", created.access.id)
        assertEquals(setOf(AppModule.BASE, AppModule.HOME, AppModule.NOTIFICATIONS), created.access.modules)
        val request = server.takeRequest()
        assertEquals("POST", request.method)
        assertEquals("/v1/access/tokens", request.path)
        val body = request.body.readUtf8()
        assertTrue(body.contains("\"name\":\"Планшет\""))
        assertTrue(body.contains("\"modules\":[\"home\",\"notifications\"]"))
        assertFalse(body.contains("\"admin\""))
    }

    @Test
    fun updatePatchesTheNameAndModulesInOneRequest() = runBlocking {
        respond(200, "updated.json")

        val updated = (client().updateToken("t-phone", "Телефон кухни", setOf(AppModule.HOME)) as Outcome.Success).value

        assertEquals("Телефон кухни", updated.name)
        assertEquals(setOf(AppModule.BASE, AppModule.HOME, AppModule.NOTIFICATIONS), updated.modules)
        val request = server.takeRequest()
        assertEquals("PATCH", request.method)
        assertEquals("/v1/access/tokens/t-phone", request.path)
        assertTrue(request.body.readUtf8().contains("\"name\":\"Телефон кухни\""))
    }

    @Test
    fun revokeDeletesTheToken() = runBlocking {
        respond(200, "revoked.json")

        val result = client().revokeToken("t-phone")

        assertEquals(Outcome.Success(Unit), result)
        val request = server.takeRequest()
        assertEquals("DELETE", request.method)
        assertEquals("/v1/access/tokens/t-phone", request.path)
    }

    @Test
    fun anHttpModuleForbiddenBecomesADedicatedFailure() = runBlocking {
        respond(403, "module_forbidden_http.json")

        val failure = (client().tokens() as Outcome.Failure).failure

        assertEquals(RemoteFailure.ModuleForbidden("admin"), failure)
    }

    @Test
    fun aConnectModuleForbiddenCarriesItsDetailModule() = runBlocking {
        respond(403, "module_forbidden_connect.json")

        val failure = (client().me() as Outcome.Failure).failure

        assertEquals(RemoteFailure.ModuleForbidden("chat"), failure)
    }

    @Test
    fun conflictLimitAndInvalidNameKeepTheirHttpStatusAndDetail() = runBlocking {
        respond(409, "name_taken.json")
        val conflict = (client().createToken("Телефон", setOf(AppModule.HOME)) as Outcome.Failure).failure
        conflict as RemoteFailure.Server
        assertEquals(409, conflict.httpStatus)
        assertEquals("conflict", conflict.reason)
        assertEquals("token name is already used", conflict.message)

        respond(429, "limit.json")
        val limit = (client().createToken("Телефон", setOf(AppModule.HOME)) as Outcome.Failure).failure
        limit as RemoteFailure.Server
        assertEquals(429, limit.httpStatus)
        assertEquals("limit", limit.reason)

        respond(400, "invalid_name.json")
        val invalid = (client().createToken("", setOf(AppModule.HOME)) as Outcome.Failure).failure
        invalid as RemoteFailure.Server
        assertEquals(400, invalid.httpStatus)
        assertEquals("invalid", invalid.reason)
        assertEquals("token name is required", invalid.message)
    }

    @Test
    fun aForbiddenReplyNotifiesTheAccessSinkSoMeIsRefreshed() = runBlocking {
        respond(403, "module_forbidden_http.json")
        val seen = mutableListOf<String>()
        val endpoint = (ServerEndpoint.parse(server.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint
        val http = testHttp(
            Store(ConnectionConfig(endpoint = endpoint, bearerToken = "tok-1")),
            forbidden = { setOf(ModuleForbiddenSink(seen::add)) },
        )

        val result = HttpAccessClient(http).tokens()

        assertTrue(result is Outcome.Failure)
        assertEquals(listOf("admin"), seen)
    }

    @Test
    fun anUnauthenticatedReplyStaysUnauthorized() = runBlocking {
        respond(401, "unauthenticated.json")

        assertEquals(RemoteFailure.Unauthorized, (client().me() as Outcome.Failure).failure)
    }
}

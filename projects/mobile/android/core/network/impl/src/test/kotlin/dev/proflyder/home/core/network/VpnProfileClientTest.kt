package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.VpnProtocolId
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class VpnProfileClientTest {
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

    private fun endpoint(): ServerEndpoint =
        (ServerEndpoint.parse(server.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint

    private fun client() = HttpVpnProfileClient(
        testHttp(Store(ConnectionConfig(endpoint = endpoint(), bearerToken = "tok-1"))),
    )

    private fun fixture(name: String): String =
        checkNotNull(javaClass.getResource("/fixtures/vpn/$name.json")).readText()

    @Test
    fun registerSendsOnlyThePublicKeyAndParsesTheProfile() = runBlocking {
        server.enqueue(MockResponse().setBody(fixture("profile")))
        val profile = (client().register("dev-1", "pub-key", "Pixel") as Outcome.Success).value
        val request = server.takeRequest()
        assertEquals("/v1/vpn/profile", request.path)
        assertEquals("Bearer tok-1", request.getHeader("Authorization"))
        assertEquals(fixture("profile_request"), request.body.readUtf8())
        assertEquals(setOf(VpnProtocolId("wireguard"), VpnProtocolId("ocserv")), profile.transports.keys)
        val wireguard = profile.settingsOf(VpnProtocolId("wireguard"))!!.json
        assertEquals("10.77.2.2/32", wireguard["interface"]!!.jsonObject["address"]!!.jsonPrimitive.content)
        assertEquals("192.0.2.29:51820", wireguard["peer"]!!.jsonObject["lanEndpoint"]!!.jsonPrimitive.content)
        assertEquals("lan", profile.settingsOf(VpnProtocolId("ocserv"))!!.json["splitGroup"]!!.jsonPrimitive.content)
        assertEquals("Pixel", profile.deviceName)
        assertEquals(1_795_100_100L, profile.updatedUnix)
    }

    @Test
    fun conflictBecomesAServerFailureWithTheHttpCode() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(409).setBody(fixture("error_conflict")))
        val failure = (client().register("dev-1", "pub-key", "Pixel") as Outcome.Failure).failure
        assertTrue(failure is RemoteFailure.Server)
        assertEquals(409, (failure as RemoteFailure.Server).httpStatus)
    }

    @Test
    fun removeReadsTheFlag() = runBlocking {
        server.enqueue(MockResponse().setBody(fixture("removed")))
        val removed = (client().remove("dev-1") as Outcome.Success).value
        assertEquals("/v1/vpn/profile/remove", server.takeRequest().path)
        assertTrue(removed)
    }
}

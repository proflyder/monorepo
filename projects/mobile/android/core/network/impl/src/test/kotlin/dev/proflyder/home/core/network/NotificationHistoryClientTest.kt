package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.NotificationHistoryType
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
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class NotificationHistoryClientTest {
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

    private fun client(): HttpNotificationHistoryApi {
        val endpoint = (ServerEndpoint.parse(server.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint
        return HttpNotificationHistoryApi(testHttp(Store(ConnectionConfig(endpoint = endpoint, bearerToken = "tok-1"))))
    }

    @Test
    fun theHistoryRequestCarriesTheDeviceHeaderAndParsesReadState() = runBlocking {
        server.enqueue(MockResponse().setBody(TestFixtures.read("history/page.json")))

        val result = client().page(null, 20)

        val page = (result as Outcome.Success).value
        assertEquals(1, page.entries.size)
        assertEquals(NotificationHistoryType.FIND, page.entries.single().type)
        assertEquals(true, page.entries.single().read)
        assertEquals(1_700_000_000L, page.entries.single().createdUnix)
        assertNull(page.nextCursor)

        val request = server.takeRequest()
        assertEquals("GET", request.method)
        assertEquals("/v1/push/history?limit=20", request.path)
        assertEquals("Bearer tok-1", request.getHeader("Authorization"))
        assertEquals(TestIdentity.ID, request.getHeader(DeviceOriginInterceptor.HEADER))
    }

    @Test
    fun theUnreadCountUsesTheSameSchedulerOrigin() = runBlocking {
        server.enqueue(MockResponse().setBody(TestFixtures.read("history/unread.json")))

        val result = client().unread()

        assertEquals(Outcome.Success(3), result)
        val request = server.takeRequest()
        assertEquals("/v1/push/history/unread-count", request.path)
        assertEquals(TestIdentity.ID, request.getHeader(DeviceOriginInterceptor.HEADER))
    }

    @Test
    fun aMissingDeviceHeaderIsReportedAsABadRequest() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(400).setBody(TestFixtures.read("history/bad_request.json")))

        val result = client().page(null, 20)

        val failure = (result as Outcome.Failure).failure
        assertTrue(failure is RemoteFailure.Server)
        assertEquals(400, (failure as RemoteFailure.Server).httpStatus)
    }
}

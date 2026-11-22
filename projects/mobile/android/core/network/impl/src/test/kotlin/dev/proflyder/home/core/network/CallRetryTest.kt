package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.core.model.isTransientNetwork
import dev.proflyder.home.core.testing.TestFixtures
import java.io.IOException
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import okhttp3.Interceptor
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.SocketPolicy
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class CallRetryTest {
    private lateinit var server: MockWebServer

    @Before
    fun setUp() {
        server = MockWebServer().apply { start() }
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    private fun endpoint(s: MockWebServer) =
        (ServerEndpoint.parse(s.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint

    private class Store(config: ConnectionConfig) : ConnectionSettingsStore {
        override val config: StateFlow<ConnectionConfig> = MutableStateFlow(config)
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.AVAILABLE)
        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }

    private val body = TestFixtures.read("catalog_empty.json")

    private val store by lazy { storeFor() }

    private fun storeFor() = Store(
        ConnectionConfig(
            endpoint = endpoint(server),
            controlEndpoint = endpoint(server),
            bearerToken = "tok-123",
        ),
    )

    private fun client(interceptors: Set<Interceptor> = emptySet()): HttpControlCatalogClient =
        HttpControlCatalogClient(testHttp(store, interceptors = interceptors))

    private fun scheduler(): ConnectSchedulerClient =
        ConnectSchedulerClient(testHttp(store), ServerClock(FakeClock()))

    @Test
    fun staleConnectionIsRetriedAndTheUserSeesSuccess() = runBlocking {
        server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AT_START))
        server.enqueue(MockResponse().setResponseCode(200).setBody(body))

        val result = client().fetch()

        assertTrue(result is Outcome.Success)
        assertEquals(2, server.requestCount)
    }

    private class CountingRouteGate : RouteGate {
        var waits = 0

        override suspend fun await() {
            waits += 1
        }
    }

    @Test
    fun aRetryAfterARouteSwitchWaitsForTheRouteAgain() = runBlocking {
        server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AT_START))
        server.enqueue(MockResponse().setResponseCode(200).setBody(body))
        val gate = CountingRouteGate()

        val result = HttpControlCatalogClient(testHttp(store, gate = gate)).fetch()

        assertTrue(result is Outcome.Success)
        assertEquals(2, gate.waits)
    }

    @Test
    fun aCancelledCallNeverBecomesAFailure() {
        val canceling = Interceptor { throw IOException("Canceled") }

        val thrown = runCatching { runBlocking { client(setOf(canceling)).fetch() } }.exceptionOrNull()

        assertTrue(thrown is CancellationException)
        assertEquals(0, server.requestCount)
    }

    @Test
    fun aRealOutageAfterRetriesIsATransientFailure() = runBlocking {
        repeat(3) { server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AT_START)) }

        val failure = (client().fetch() as Outcome.Failure).failure

        assertTrue(failure is RemoteFailure.Offline)
        assertTrue(failure.isTransientNetwork())
        assertEquals(3, server.requestCount)
    }

    @Test
    fun aWriteIsNotRetriedOnATransportFailure() = runBlocking {
        server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AT_START))
        server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AT_START))
        server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AT_START))

        val failure = (scheduler().enqueueTask("t", "q", "{}", "key") as Outcome.Failure).failure

        assertTrue(failure is RemoteFailure.Offline)
        assertTrue(server.requestCount < 3)
    }

    @Test
    fun theRouteGateIsAwaitedBeforeTheRequest() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody(body))
        val gate = HoldingRouteGate()
        val pending = async(Dispatchers.IO) { HttpControlCatalogClient(testHttp(store, gate = gate)).fetch() }

        gate.awaited.await()
        assertEquals(0, server.requestCount)
        gate.open.complete(Unit)

        assertTrue(pending.await() is Outcome.Success)
        assertEquals(1, server.requestCount)
    }

    private class HoldingRouteGate : RouteGate {
        val awaited = CompletableDeferred<Unit>()
        val open = CompletableDeferred<Unit>()

        override suspend fun await() {
            awaited.complete(Unit)
            open.await()
        }
    }
}

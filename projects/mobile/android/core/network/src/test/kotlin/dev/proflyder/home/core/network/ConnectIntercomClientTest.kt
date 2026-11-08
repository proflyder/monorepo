package dev.proflyder.home.core.network

import dev.proflyder.home.core.testing.TestFixtures
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.IntercomCallState
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.IntercomOutgoingSignal
import dev.proflyder.home.core.model.IntercomSdpSignal
import dev.proflyder.home.core.model.IntercomSdpType
import dev.proflyder.home.core.model.IntercomWatchEvent
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ConnectIntercomClientTest {
    private lateinit var server: MockWebServer
    private lateinit var store: FakeIntercomSettingsStore
    private val json = Json { ignoreUnknownKeys = true }

    @Before
    fun setUp() {
        server = MockWebServer().apply { start() }
        val base = server.url("/").toString().removeSuffix("/")
        store = FakeIntercomSettingsStore(
            ConnectionConfig(
                intercomEndpoint = (ServerEndpoint.parse(base) as EndpointParseResult.Valid).endpoint,
                bearerToken = "phone-token",
            ),
        )
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    private fun client() = ConnectIntercomClient(settings = store, baseClient = OkHttpClient())

    @Test
    fun startCallUsesIntercomPathBearerAndIdempotencyKey() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200).setBody(
                TestFixtures.read("intercom_call_ringing.json"),
            ),
        )

        val result = client().startCall("station", "key-1")
        assertTrue(result is Outcome.Success)
        assertEquals("c-1", (result as Outcome.Success).value.id)
        assertEquals(IntercomCallState.RINGING, result.value.state)
        assertEquals("Station", result.value.callee?.displayName)

        val request = server.takeRequest()
        assertEquals("/home.v1.IntercomService/StartCall", request.path)
        assertEquals("Bearer phone-token", request.getHeader("Authorization"))
        assertEquals("1", request.getHeader("Connect-Protocol-Version"))
        val body = json.parseToJsonElement(request.body.readUtf8()).jsonObject
        assertEquals("station", body["calleeEndpointId"]!!.jsonPrimitive.content)
        assertEquals("key-1", body["idempotencyKey"]!!.jsonPrimitive.content)
    }

    @Test
    fun hangupCarriesReasonAndSendSignalCarriesSdp() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody(TestFixtures.read("intercom_call_minimal.json")))
        server.enqueue(MockResponse().setResponseCode(200).setBody(TestFixtures.read("intercom_delivered.json")))

        client().hangup("c-1", IntercomEndReason.CANCELED)
        val hungUp = json.parseToJsonElement(server.takeRequest().body.readUtf8()).jsonObject
        assertEquals("c-1", hungUp["callId"]!!.jsonPrimitive.content)
        assertEquals("END_REASON_CANCELED", hungUp["reason"]!!.jsonPrimitive.content)

        val delivered = client().sendSignal(
            "c-1",
            IntercomOutgoingSignal.Sdp(IntercomSdpSignal(IntercomSdpType.OFFER, "v=0")),
        )
        assertEquals(true, (delivered as Outcome.Success).value)
        val signal = json.parseToJsonElement(server.takeRequest().body.readUtf8()).jsonObject
        val sdp = signal["sdp"]!!.jsonObject
        assertEquals("SDP_TYPE_OFFER", sdp["type"]!!.jsonPrimitive.content)
        assertEquals("v=0", sdp["sdp"]!!.jsonPrimitive.content)
    }

    @Test
    fun iceConfigEmptyMeansHostCandidates() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody(TestFixtures.read("intercom_ice_empty.json")))
        val result = client().getIceConfig()
        assertEquals(emptyList<Any>(), (result as Outcome.Success).value)
    }

    @Test
    fun unauthorizedIsReportedWithoutRetry() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(401).setBody(TestFixtures.read("error_unauthenticated.json")))
        assertEquals(RemoteFailure.Unauthorized, client().listEndpoints().failureOrNull())
    }

    @Test
    fun watchParsesConnectEnvelopeFrames() = runBlocking {
        val first = TestFixtures.read("intercom_state_changed.json")
        val second = TestFixtures.read("intercom_sdp.json")
        val body = Buffer()
            .write(ConnectFrames.encode(first))
            .write(ConnectFrames.encode(second))
        server.enqueue(
            MockResponse()
                .setResponseCode(200)
                .setHeader("Content-Type", "application/connect+json")
                .setBody(body),
        )

        val events = withTimeout(5_000) { client().watch().take(2).toList() }

        assertEquals(2, events.size)
        val call = events[0] as IntercomWatchEvent.CallChanged
        assertEquals("c-1", call.call.id)
        assertEquals(IntercomCallState.CONNECTING, call.call.state)
        assertEquals(IntercomCallState.RINGING, call.previousState)
        val sdp = events[1] as IntercomWatchEvent.Sdp
        assertEquals(IntercomSdpType.ANSWER, sdp.signal.type)
        assertEquals("v=0", sdp.signal.sdp)

        val request = server.takeRequest()
        assertEquals("/home.v1.IntercomService/Watch", request.path)
        assertEquals("Bearer phone-token", request.getHeader("Authorization"))
        assertEquals("application/connect+json", request.getHeader("Content-Type"))
        // The request is a single Connect envelope carrying an (empty) WatchRequest.
        val raw = request.body.readByteArray()
        val frame = json.parseToJsonElement(raw.copyOfRange(5, raw.size).decodeToString()).jsonObject
        assertTrue(frame.isEmpty())
    }

    @Test
    fun watchReconnectsAfterStreamEnd() = runBlocking {
        val event = TestFixtures.read("intercom_ice_end.json")
        repeat(2) {
            server.enqueue(
                MockResponse()
                    .setResponseCode(200)
                    .setHeader("Content-Type", "application/connect+json")
                    .setBody(Buffer().write(ConnectFrames.encode(event))),
            )
        }

        val events = withTimeout(10_000) { client().watch().take(2).toList() }

        assertEquals(2, events.size)
        assertTrue(events.all { it is IntercomWatchEvent.IceEnd })
        assertEquals(2, server.requestCount)
    }

    @Test
    fun missingTokenFailsBeforeAnyRequest() = runBlocking {
        store = FakeIntercomSettingsStore(
            ConnectionConfig(
                intercomEndpoint = (ServerEndpoint.parse(server.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint,
                bearerToken = null,
            ),
        )
        assertEquals(
            RemoteFailure.Validation(dev.proflyder.home.core.model.Field.TOKEN),
            client().listEndpoints().failureOrNull(),
        )
        assertEquals(0, server.requestCount)
        assertFalse(store.config.value.hasToken)
    }

    private class FakeIntercomSettingsStore(initial: ConnectionConfig) : ConnectionSettingsStore {
        private val mutable = MutableStateFlow(initial)
        override val config: StateFlow<ConnectionConfig> = mutable
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.ABSENT)

        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }
}

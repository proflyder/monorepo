package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.ServerClock
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ConnectSchedulerClientTest {
    private lateinit var server: MockWebServer
    private lateinit var otherServer: MockWebServer
    private lateinit var store: FakeSettingsStore
    private lateinit var clock: ServerClock
    private var nowMillis = 1_000_000L

    private val json = Json { ignoreUnknownKeys = true }

    @Before
    fun setUp() {
        server = MockWebServer().apply { start() }
        otherServer = MockWebServer().apply { start() }
        store = FakeSettingsStore(defaultConfig())
        clock = ServerClock { nowMillis }
    }

    @After
    fun tearDown() {
        server.shutdown()
        otherServer.shutdown()
    }

    private fun endpoint(path: String = ""): ServerEndpoint =
        (ServerEndpoint.parse("${server.url("/")}${path.removePrefix("/")}".removeSuffix("/")) as EndpointParseResult.Valid)
            .endpoint

    private fun defaultConfig(
        path: String = "",
        token: String? = "test-token",
        connectMillis: Long = 2_000,
        readMillis: Long = 2_000,
    ): ConnectionConfig = ConnectionConfig(
        endpoint = endpoint(path),
        bearerToken = token,
        connectTimeoutMillis = connectMillis,
        readTimeoutMillis = readMillis,
    )

    private fun client(): ConnectSchedulerClient = ConnectSchedulerClient(
        settings = store,
        baseClient = OkHttpClient.Builder()
            .followRedirects(true)
            .followSslRedirects(true)
            .build(),
        serverClock = clock,
    )

    @Test
    fun getStatusUsesConnectJsonHeadersAndSyncsServerClock() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200).setBody(
                """{"version":"1.2.3","schemaVersion":"4","nowUnix":"5000","lastTickUnix":"4999",
                   "pendingEvents":2,"leasedEvents":1,"enabledSchedules":3,"runningTimers":4,
                   "consumers":[{"consumerId":"ha","lastSeenUnix":"4000","lastAckUnix":"3900","alive":true}],
                   "scenarioExecutionSupported":false,"queues":[]}""",
            ),
        )

        val result = client().getStatus()
        assertTrue(result is Outcome.Success)
        val status = (result as Outcome.Success).value
        assertEquals("1.2.3", status.version)
        assertEquals(4, status.schemaVersion)
        assertEquals(5_000L, status.nowUnix)
        assertEquals(1, status.consumers.size)
        assertTrue(status.consumers.first().alive)
        assertTrue(clock.isSynced)
        assertEquals(5_000L, clock.nowUnix())

        val request = server.takeRequest(2, TimeUnit.SECONDS)!!
        assertEquals("POST", request.method)
        assertEquals("/ricardo.v1.SchedulerService/GetStatus", request.path)
        assertEquals("application/json", request.getHeader("Content-Type"))
        assertEquals("1", request.getHeader("Connect-Protocol-Version"))
        assertEquals("Bearer test-token", request.getHeader("Authorization"))
        assertEquals("{}", request.body.readUtf8())
        assertFalse(request.path!!.contains("test-token"))
    }

    @Test
    fun createScheduleSendsProtoLowerCamelCaseAndEnumNames() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200).setBody(
                """{"schedule":{"id":"s1","label":"Wake up","kind":"SCHEDULE_KIND_ALARM",
                   "timezone":"Asia/Almaty","at":{"hour":7,"minute":30,"second":0},
                   "repeat":"REPEAT_KIND_CUSTOM","weekdays":[1,3,5],"enabled":true,
                   "nextDueUnix":"9000","revision":"2"}}""",
            ),
        )
        val draft = dev.proflyder.home.core.model.ScheduleDraft(
            label = "Wake up",
            kind = ScheduleKind.ALARM,
            timezone = "Asia/Almaty",
            at = dev.proflyder.home.core.model.WallTime(7, 30),
            repeat = dev.proflyder.home.core.model.RepeatKind.CUSTOM,
            weekdays = listOf(5, 1, 3),
            enabled = true,
            idempotencyKey = "key-1",
        )

        val result = client().createSchedule(draft)
        assertTrue(result is Outcome.Success)
        val schedule = (result as Outcome.Success).value
        assertEquals("s1", schedule.id)
        assertEquals(ScheduleKind.ALARM, schedule.kind)
        assertEquals(listOf(1, 3, 5), schedule.weekdays)

        val body = json.parseToJsonElement(server.takeRequest().body.readUtf8()).jsonObject
        assertEquals("SCHEDULE_KIND_ALARM", body["kind"]!!.jsonPrimitive.content)
        assertEquals("REPEAT_KIND_CUSTOM", body["repeat"]!!.jsonPrimitive.content)
        assertEquals("Asia/Almaty", body["timezone"]!!.jsonPrimitive.content)
        assertEquals(listOf(1, 3, 5), body["weekdays"]!!.jsonArray.map { it.jsonPrimitive.content.toInt() })
        assertEquals(7, body["at"]!!.jsonObject["hour"]!!.jsonPrimitive.content.toInt())
        assertEquals("key-1", body["idempotencyKey"]!!.jsonPrimitive.content)
        assertNull(body["onceDate"])
    }

    @Test
    fun startTimerEncodesInt64AsString() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200).setBody(
                """{"timer":{"id":"t1","label":"Tea","durationSeconds":"90","remainingSeconds":"90",
                   "dueUnix":"6000","state":"TIMER_STATE_RUNNING","revision":"1"}}""",
            ),
        )
        val result = client().startTimer("Tea", 90, "idem")
        assertTrue(result is Outcome.Success)
        val timer = (result as Outcome.Success).value
        assertEquals(90L, timer.durationSeconds)
        assertEquals(dev.proflyder.home.core.model.TimerState.RUNNING, timer.state)

        val body = json.parseToJsonElement(server.takeRequest().body.readUtf8()).jsonObject
        assertTrue(body["durationSeconds"]!!.jsonPrimitive.isString)
        assertEquals("90", body["durationSeconds"]!!.jsonPrimitive.content)
    }

    @Test
    fun updateScheduleSendsExpectedRevisionAsString() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200).setBody(
                """{"schedule":{"id":"s1","label":"Wake up","kind":"SCHEDULE_KIND_ALARM",
                   "timezone":"Asia/Almaty","at":{"hour":8,"minute":0},"repeat":"REPEAT_KIND_DAILY",
                   "enabled":true,"revision":"3"}}""",
            ),
        )
        val existing = dev.proflyder.home.core.model.Schedule(
            id = "s1",
            label = "Wake up",
            kind = ScheduleKind.ALARM,
            timezone = "Asia/Almaty",
            at = dev.proflyder.home.core.model.WallTime(7, 0),
            onceDate = "",
            repeat = dev.proflyder.home.core.model.RepeatKind.DAILY,
            weekdays = emptyList(),
            nextDueUnix = 0,
            enabled = true,
            snoozeUntilUnix = 0,
            lastFiredUnix = 0,
            snoozeCount = 0,
            revision = 2,
            createdUnix = 0,
            updatedUnix = 0,
        )
        val result = client().updateSchedule(existing.copy(at = dev.proflyder.home.core.model.WallTime(8, 0)), 2)
        assertTrue(result is Outcome.Success)

        val body = json.parseToJsonElement(server.takeRequest().body.readUtf8()).jsonObject
        assertEquals("2", body["expectedRevision"]!!.jsonPrimitive.content)
        assertEquals("2", body["schedule"]!!.jsonObject["revision"]!!.jsonPrimitive.content)
        assertEquals(8, body["schedule"]!!.jsonObject["at"]!!.jsonObject["hour"]!!.jsonPrimitive.content.toInt())
    }

    @Test
    fun unauthorizedIsDistinctFromOffline() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(401).setBody("""{"code":"unauthenticated","message":"bad token"}"""))
        assertEquals(RemoteFailure.Unauthorized, client().getStatus().failureOrNull())
    }

    @Test
    fun connectErrorCodesAreMapped() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(503).setBody(
                """{"code":"unavailable","message":"scheduler ticking"}""",
            ),
        )
        val failure = client().getStatus().failureOrNull() as RemoteFailure.Offline
        assertEquals("scheduler ticking", failure.detail)
    }

    @Test
    fun redirectsAreNotFollowedAndBearerIsNeverForwarded() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(302)
                .setHeader("Location", otherServer.url("/steal")),
        )
        val result = client().getStatus()
        assertTrue(result is Outcome.Failure)
        val failure = (result as Outcome.Failure).failure
        assertTrue(failure is RemoteFailure.ProtocolFailure)
        assertTrue((failure as RemoteFailure.ProtocolFailure).detail.contains("302"))
        assertNull(otherServer.takeRequest(300, TimeUnit.MILLISECONDS))
    }

    @Test
    fun missingEndpointMakesNoRequest() = runBlocking {
        store.set(ConnectionConfig(endpoint = null))
        val result = client().getStatus()
        assertEquals(RemoteFailure.NotConfigured, result.failureOrNull())
        assertNull(server.takeRequest(200, TimeUnit.MILLISECONDS))
    }

    @Test
    fun malformedResponseIsProtocolFailure() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody("{not-json"))
        assertTrue(client().getStatus().failureOrNull() is RemoteFailure.ProtocolFailure)
    }

    @Test
    fun timeoutIsReportedAsTimeout() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200)
                .setBody("{}")
                .setBodyDelay(1_300, TimeUnit.MILLISECONDS),
        )
        // Invalid configured timeouts are clamped to the safe minimum, so use
        // the minimum read timeout and delay beyond it.
        store.set(defaultConfig(readMillis = 1))
        assertEquals(RemoteFailure.Timeout, client().getStatus().failureOrNull())
    }

    @Test
    fun connectionRefusedIsOffline() = runBlocking {
        val dead = MockWebServer().apply { start() }
        val url = dead.url("/")
        dead.shutdown()
        store.set(ConnectionConfig(
            endpoint = (ServerEndpoint.parse(url.toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint,
            bearerToken = "test-token",
        ))
        assertTrue(client().getStatus().failureOrNull() is RemoteFailure.Offline)
    }

    @Test
    fun blankTokenIsRejectedBeforeAnyRequest() = runBlocking {
        store.set(defaultConfig(token = "  "))
        val result = client().getStatus()
        assertEquals(RemoteFailure.Validation(Field.TOKEN), result.failureOrNull())
        assertNull(server.takeRequest(200, TimeUnit.MILLISECONDS))
    }

    @Test
    fun controlCharactersInTokenAreRejectedBeforeRequest() = runBlocking {
        store.set(defaultConfig(token = "bad\ntoken"))
        val result = client().getStatus()
        assertEquals(
            RemoteFailure.Validation(Field.TOKEN),
            result.failureOrNull(),
        )
        assertNull(server.takeRequest(200, TimeUnit.MILLISECONDS))
    }

    @Test
    fun headerInjectionTokenIsRejectedBeforeAnyRequest() = runBlocking {
        store.set(defaultConfig(token = "token\r\nX-Evil: 1"))
        val result = client().getStatus()
        assertEquals(RemoteFailure.Validation(Field.TOKEN), result.failureOrNull())
        assertNull(server.takeRequest(200, TimeUnit.MILLISECONDS))
    }

    @Test
    fun malformedPresentListFieldIsProtocolFailureButMissingFieldIsEmpty() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"schedules":{}}"""))
        val malformedSchedules = client().listSchedules(includeDisabled = true, kind = null)
        assertTrue(malformedSchedules.toString(), malformedSchedules.failureOrNull() is RemoteFailure.ProtocolFailure)

        server.enqueue(MockResponse().setResponseCode(200).setBody("""{}"""))
        val emptySchedules = client().listSchedules(includeDisabled = true, kind = null)
        assertTrue(emptySchedules is Outcome.Success)
        assertTrue((emptySchedules as Outcome.Success).value.isEmpty())

        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"timers":null}"""))
        assertTrue(client().listTimers(includeFinished = false).failureOrNull() is RemoteFailure.ProtocolFailure)

        server.enqueue(MockResponse().setResponseCode(200).setBody("""{}"""))
        val emptyTimers = client().listTimers(includeFinished = false)
        assertTrue(emptyTimers is Outcome.Success)
        assertTrue((emptyTimers as Outcome.Success).value.isEmpty())
    }

    @Test
    fun oversizedResponseIsRejectedWithoutParsing() = runBlocking {
        val oversized = "x".repeat(8 * 1024 * 1024 + 1)
        server.enqueue(MockResponse().setResponseCode(200).setBody(oversized))
        val failure = client().getStatus().failureOrNull()
        assertTrue(failure is RemoteFailure.ProtocolFailure)
        assertTrue((failure as RemoteFailure.ProtocolFailure).detail.contains("8 MiB"))
    }

    @Test
    fun basePathPrefixIsPreserved() = runBlocking {
        store.set(defaultConfig(path = "api"))
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"nowUnix":"1"}"""))
        client().getStatus()
        assertEquals("/api/ricardo.v1.SchedulerService/GetStatus", server.takeRequest().path)
    }

    @Test
    fun listSchedulesMapsSchedulesAndFiltersKind() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200).setBody(
                """{"schedules":[{"id":"s1","label":"A","kind":"SCHEDULE_KIND_ALARM",
                   "timezone":"UTC","at":{"hour":9,"minute":0},"repeat":"REPEAT_KIND_DAILY",
                   "enabled":true,"nextDueUnix":"123","snoozeUntilUnix":"0","revision":"1"}]}""",
            ),
        )
        val result = client().listSchedules(includeDisabled = true, kind = ScheduleKind.ALARM)
        assertTrue(result is Outcome.Success)
        assertEquals(1, (result as Outcome.Success).value.size)
        val body = json.parseToJsonElement(server.takeRequest().body.readUtf8()).jsonObject
        assertEquals("true", body["includeDisabled"]!!.jsonPrimitive.content)
        assertEquals("SCHEDULE_KIND_ALARM", body["kind"]!!.jsonPrimitive.content)
    }

    @Test
    fun unauthorizedServerErrorBodyWithoutJsonStillMapsFromStatus() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(403))
        assertEquals(RemoteFailure.Unauthorized, client().getStatus().failureOrNull())
    }

    @Test
    fun requestBodyDoesNotContainToken() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"nowUnix":"1"}"""))
        client().getStatus()
        val request = server.takeRequest()
        assertFalse(request.body.readUtf8().contains("test-token"))
        assertFalse(request.path!!.contains("test-token"))
    }

    private class FakeSettingsStore(initial: ConnectionConfig) : ConnectionSettingsStore {
        override val config: StateFlow<ConnectionConfig> = MutableStateFlow(initial)
        override val tokenStatus: StateFlow<TokenStatus> =
            MutableStateFlow(if (initial.hasToken) TokenStatus.AVAILABLE else TokenStatus.ABSENT)

        private val mutable = config as MutableStateFlow<ConnectionConfig>

        fun set(config: ConnectionConfig) {
            mutable.value = config
        }

        override suspend fun update(config: ConnectionConfig): Outcome<Unit> {
            mutable.value = config
            return Outcome.Success(Unit)
        }
    }
}

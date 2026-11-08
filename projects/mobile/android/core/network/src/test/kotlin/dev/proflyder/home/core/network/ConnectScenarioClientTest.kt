package dev.proflyder.home.core.network

import dev.proflyder.home.core.testing.TestFixtures
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
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

class ConnectScenarioClientTest {
    private lateinit var server: MockWebServer
    private lateinit var store: ScenarioFakeSettingsStore
    private val json = Json { ignoreUnknownKeys = true }

    @Before
    fun setUp() {
        server = MockWebServer().apply { start() }
        store = ScenarioFakeSettingsStore(defaultConfig())
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    private fun defaultConfig(): ConnectionConfig = ConnectionConfig(
        endpoint = (ServerEndpoint.parse(server.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint,
        bearerToken = "test-token",
        connectTimeoutMillis = 2_000,
        readTimeoutMillis = 2_000,
    )

    private fun client(): ConnectScenarioClient = ConnectScenarioClient(
        settings = store,
        baseClient = OkHttpClient(),
    )

    private fun scenarioJson(id: String = "sc-1"): String =
        """{"scenario":{"id":"$id","label":"Evening","description":"","enabled":true,
           "revision":"3","createdUnix":"1","updatedUnix":"2",
           "triggers":[{"manual":{}},{"time":{"timezone":"Asia/Almaty",
              "at":{"hour":19,"minute":30},"repeat":"REPEAT_KIND_WEEKDAYS"}}],
           "conditions":[{"entityState":{"entityId":"light.kitchen","state":"on"}}],
           "steps":[{"catalog":{"entryId":"light.kitchen","action":"set","paramsJson":"{}"}},
                    {"speak":{"text":"Hello"}},{"delay":{"seconds":5}}],
           "continueOnError":false,"nextDueUnix":"123"}}"""

    @Test
    fun listUsesScenarioServicePathAndParsesAllVariants() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200).setBody(
                TestFixtures.read("scenario_list.json"),
            ),
        )
        val result = client().listScenarios(includeDisabled = true)
        assertTrue(result is Outcome.Success)
        val scenario = (result as Outcome.Success).value.single()
        assertEquals(2, scenario.triggers.size)
        assertEquals(RepeatKind.DAILY, (scenario.triggers[1] as ScenarioTrigger.Time).trigger.repeat)
        assertEquals(2, scenario.conditions.size)
        assertEquals(3, scenario.steps.size)
        assertEquals(ScenarioStep.Delay(5), scenario.steps[2])

        val request = server.takeRequest(2, TimeUnit.SECONDS)!!
        assertEquals("/home.v1.ScenarioService/ListScenarios", request.path)
        assertEquals("Bearer test-token", request.getHeader("Authorization"))
        val body = json.parseToJsonElement(request.body.readUtf8()).jsonObject
        assertEquals("true", body["includeDisabled"]!!.jsonPrimitive.content)
    }

    @Test
    fun createSendsOneofAndIdempotencyKey() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody(scenarioJson()))
        val draft = ScenarioDraft(
            label = "Morning",
            triggers = listOf(
                ScenarioTrigger.Time(
                    TimeTrigger("Asia/Almaty", WallTime(7, 30), RepeatKind.WEEKDAYS),
                ),
            ),
            steps = listOf(ScenarioStep.Speak("Wake up")),
            idempotencyKey = "key-1",
        )
        assertTrue(client().createScenario(draft) is Outcome.Success)
        val request = server.takeRequest(2, TimeUnit.SECONDS)!!
        assertEquals("/home.v1.ScenarioService/CreateScenario", request.path)
        val body = json.parseToJsonElement(request.body.readUtf8()).jsonObject
        assertEquals("key-1", body["idempotencyKey"]!!.jsonPrimitive.content)
        val trigger = body.getValue("triggers").jsonArray.single().jsonObject
        assertEquals("7", trigger.getValue("time").jsonObject.getValue("at").jsonObject["hour"]!!.jsonPrimitive.content)
        assertEquals("Wake up", body.getValue("steps").jsonArray.single().jsonObject
            .getValue("speak").jsonObject["text"]!!.jsonPrimitive.content)
    }

    @Test
    fun updateSendsExpectedRevisionAsString() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody(scenarioJson()))
        val parsed = ScenarioJson.parseScenario(
            json.parseToJsonElement(scenarioJson()).jsonObject.getValue("scenario"),
        )
        assertTrue(client().updateScenario(parsed.copy(label = "New"), expectedRevision = 3) is Outcome.Success)
        val body = json.parseToJsonElement(server.takeRequest().body.readUtf8()).jsonObject
        assertEquals("3", body["expectedRevision"]!!.jsonPrimitive.content)
        assertEquals("3", body["scenario"]!!.jsonObject["revision"]!!.jsonPrimitive.content)
        assertEquals("New", body["scenario"]!!.jsonObject["label"]!!.jsonPrimitive.content)
    }

    @Test
    fun runReturnsEventIdAndSendsIdempotencyKey() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(200)
                .setBody(TestFixtures.read("event_pending.json")),
        )
        val result = client().runScenario("sc-1", "run-key")
        assertEquals("evt-1", (result as Outcome.Success).value)
        val request = server.takeRequest(2, TimeUnit.SECONDS)!!
        assertEquals("/home.v1.ScenarioService/RunScenario", request.path)
        val body = json.parseToJsonElement(request.body.readUtf8()).jsonObject
        assertEquals("sc-1", body["id"]!!.jsonPrimitive.content)
        assertEquals("run-key", body["idempotencyKey"]!!.jsonPrimitive.content)
    }

    @Test
    fun revisionConflictIsAServerFailureWithAbortedCode() = runBlocking {
        server.enqueue(
            MockResponse().setResponseCode(409)
                .setBody(TestFixtures.read("error_aborted.json")),
        )
        val failure = client().updateScenario(
            ScenarioJson.parseScenario(json.parseToJsonElement(scenarioJson()).jsonObject.getValue("scenario")),
            2,
        ).failureOrNull()
        assertEquals("aborted", (failure as RemoteFailure.Server).code)
    }

    @Test
    fun redirectIsNeverFollowedAndTokenStaysInHeader() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(302).setHeader("Location", "http://192.0.2.10/steal"))
        val failure = client().listScenarios(true).failureOrNull()
        assertTrue(failure is RemoteFailure.ProtocolFailure)
        val request = server.takeRequest(2, TimeUnit.SECONDS)!!
        assertEquals("Bearer test-token", request.getHeader("Authorization"))
        assertFalse(request.path!!.contains("test-token"))
    }

    @Test
    fun malformedListIsProtocolFailureButMissingListIsEmpty() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(200).setBody(TestFixtures.read("scenarios_object.json")))
        assertTrue(client().listScenarios(true).failureOrNull() is RemoteFailure.ProtocolFailure)

        server.enqueue(MockResponse().setResponseCode(200).setBody("""{}"""))
        val empty = client().listScenarios(true)
        assertTrue(empty is Outcome.Success)
        assertTrue((empty as Outcome.Success).value.isEmpty())
    }

    @Test
    fun missingEndpointMakesNoRequest() = runBlocking {
        store.set(ConnectionConfig(endpoint = null))
        assertEquals(RemoteFailure.NotConfigured, client().listScenarios(true).failureOrNull())
        assertNull(server.takeRequest(200, TimeUnit.MILLISECONDS))
    }

    private class ScenarioFakeSettingsStore(initial: ConnectionConfig) : ConnectionSettingsStore {
        override val config: StateFlow<ConnectionConfig> = MutableStateFlow(initial)
        override val tokenStatus: StateFlow<TokenStatus> =
            MutableStateFlow(if (initial.hasToken) TokenStatus.AVAILABLE else TokenStatus.ABSENT)

        private val mutable = config as MutableStateFlow<ConnectionConfig>

        fun set(config: ConnectionConfig) {
            mutable.value = config
        }

        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> {
            mutable.value = mutable.value.copy(bearerToken = bearerToken)
            return Outcome.Success(Unit)
        }

    }
}

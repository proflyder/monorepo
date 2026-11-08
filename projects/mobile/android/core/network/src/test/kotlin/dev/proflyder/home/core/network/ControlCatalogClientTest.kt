package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
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

class ControlCatalogClientTest {
    private lateinit var server: MockWebServer
    private lateinit var other: MockWebServer

    @Before
    fun setUp() {
        server = MockWebServer().apply { start() }
        other = MockWebServer().apply { start() }
    }

    @After
    fun tearDown() {
        server.shutdown()
        other.shutdown()
    }

    private fun endpoint(s: MockWebServer) =
        (ServerEndpoint.parse(s.url("/").toString().removeSuffix("/")) as EndpointParseResult.Valid).endpoint

    private class Store(config: ConnectionConfig) : ConnectionSettingsStore {
        override val config: StateFlow<ConnectionConfig> = MutableStateFlow(config)
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.AVAILABLE)
        override val intercomTokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.AVAILABLE)
        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
        override suspend fun updateIntercomToken(intercomToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }

    private fun client(control: ServerEndpoint? = endpoint(server), token: String? = "tok-123") =
        HttpControlCatalogClient(
            Store(ConnectionConfig(endpoint = endpoint(other), controlEndpoint = control, bearerToken = token)),
            OkHttpClient(),
        )

    private val body = """
        {"statesOk":true,"entries":[
         {"id":"voice","kind":"profile","title":"Voice profile","actions":["turn_on","turn_off"],"state":"on","available":true},
         {"id":"focus","kind":"scenario","title":"Focus","icon":"work","actions":["run"],"available":true},
         {"id":"lamp","kind":"light","title":"Lamp","room":"Bedroom","actions":["turn_on","turn_off","bogus"],"state":"on","available":true},
         {"id":"action.station_speak","kind":"action","title":"Сказать на колонке","device_id":"station-1","device":"Колонка","actions":["speak"],"available":true},
         {"id":"odd","kind":"lock","title":"Odd","actions":[],"available":false}]}
    """.trimIndent()

    @Test
    fun fetchesAndParsesCatalogWithBearer() = runBlocking {
        server.enqueue(MockResponse().setBody(body))
        val result = client().fetch()
        val catalog = (result as Outcome.Success).value
        assertTrue(catalog.statesOk)
        assertEquals(5, catalog.entries.size)
        val (voice, focus, lamp, speak, odd) = catalog.entries
        assertEquals(ControlKind.PROFILE, voice.kind)
        assertTrue(voice.isProfile)
        assertFalse(voice.isDevice)
        assertEquals(PowerState.ON, voice.power)
        assertEquals(setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF), voice.actions)
        assertEquals(ControlKind.SCENARIO, focus.kind)
        assertEquals(setOf(ControlAction.RUN), focus.actions)
        assertNull(focus.power)
        assertEquals(ControlKind.LIGHT, lamp.kind)
        assertEquals(PowerState.ON, lamp.power)
        assertEquals("Bedroom", lamp.room)
        assertEquals(setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF), lamp.actions)
        assertEquals(ControlKind.ACTION, speak.kind)
        assertEquals(setOf(ControlAction.SPEAK), speak.actions)
        assertEquals("station-1", speak.deviceId)
        assertNull(speak.power)
        assertEquals(ControlKind.OTHER, odd.kind)
        assertFalse(odd.available)

        val request = server.takeRequest()
        assertEquals("GET", request.method)
        assertEquals("/v1/catalog", request.path)
        assertEquals("Bearer tok-123", request.getHeader("Authorization"))
    }

    @Test
    fun missingControlEndpointIsAControlledFailure() = runBlocking {
        val result = client(control = null).fetch()
        assertEquals(
            RemoteFailure.Validation(Field.CONTROL_ENDPOINT),
            (result as Outcome.Failure).failure,
        )
        assertEquals(0, server.requestCount)
    }

    @Test
    fun missingTokenFailsBeforeAnyRequest() = runBlocking {
        val result = client(token = null).fetch()
        assertEquals(RemoteFailure.Validation(Field.TOKEN), (result as Outcome.Failure).failure)
        assertEquals(0, server.requestCount)
    }

    @Test
    fun unauthorizedAndServerErrorsAreMapped() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(401))
        assertEquals(RemoteFailure.Unauthorized, (client().fetch() as Outcome.Failure).failure)
        server.enqueue(MockResponse().setResponseCode(502))
        assertTrue((client().fetch() as Outcome.Failure).failure is RemoteFailure.Server)
    }

    @Test
    fun redirectsAreRefusedAndNeverFollowed() = runBlocking {
        other.enqueue(MockResponse().setBody(body))
        server.enqueue(MockResponse().setResponseCode(302).setHeader("Location", other.url("/v1/catalog").toString()))
        val failure = (client().fetch() as Outcome.Failure).failure
        assertTrue(failure is RemoteFailure.ProtocolFailure)
        assertEquals(0, other.requestCount)
    }

    @Test
    fun malformedBodyIsAProtocolFailure() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"entries":"nope"}"""))
        assertTrue((client().fetch() as Outcome.Failure).failure is RemoteFailure.ProtocolFailure)
        server.enqueue(MockResponse().setBody("not json"))
        assertTrue((client().fetch() as Outcome.Failure).failure is RemoteFailure.ProtocolFailure)
    }

    @Test
    fun parsesOptionalDeviceGroupingWithSafeDefaults() {
        val catalog = ControlJson.parseCatalog(
            kotlinx.serialization.json.Json.parseToJsonElement(
                """{"statesOk":true,"entries":[
                  {"id":"lamp","kind":"light","title":"Lamp","device_id":"dev.lamp","device":"Ceiling lamp","role":"primary","available":true},
                  {"id":"lamp.night","kind":"switch","title":"Night light","device_id":"dev.lamp","device":"Ceiling lamp","role":"setting","available":true},
                  {"id":"legacy","kind":"switch","title":"Legacy","available":true}]}""",
            ),
        )
        val (lamp, night, legacy) = catalog.entries
        assertEquals("dev.lamp", lamp.deviceId)
        assertEquals("Ceiling lamp", lamp.device)
        assertEquals(ControlRole.PRIMARY, lamp.role)
        assertEquals("dev.lamp", night.deviceId)
        assertEquals(ControlRole.SETTING, night.role)
        // An older server omits the fields: every entry stays its own primary device.
        assertEquals("", legacy.deviceId)
        assertEquals("", legacy.device)
        assertEquals(ControlRole.PRIMARY, legacy.role)
    }

    @Test
    fun parsesLightCapabilitiesAndBuildsAdjustPayload() {
        val catalog = ControlJson.parseCatalog(
            kotlinx.serialization.json.Json.parseToJsonElement(
                """{"statesOk":true,"entries":[
                  {"id":"light.strip","kind":"light","title":"Strip","actions":["turn_on","turn_off","set"],"state":"on","available":true,
                   "light":{"brightness":true,"colorTemp":true,"color":false,"minKelvin":2700,"maxKelvin":6500,"brightnessPct":40,"kelvin":3000}},
                  {"id":"light.plain","kind":"light","title":"Plain","actions":["turn_on","turn_off"],"available":true}]}""",
            ),
        )
        val strip = catalog.entries[0].light!!
        assertTrue(strip.brightness && strip.colorTemp && !strip.color)
        assertEquals(2700, strip.minKelvin)
        assertEquals(40, strip.brightnessPct)
        assertEquals(setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF, ControlAction.SET), catalog.entries[0].actions)
        assertNull(catalog.entries[1].light)
        assertEquals(
            """{"entry":"light.strip","action":"set","brightness":55,"hue":120,"saturation":80}""",
            ControlJson.adjustPayload(
                "light.strip",
                dev.proflyder.home.core.model.LightAdjust(brightnessPct = 55, hue = 120, saturation = 80),
            ),
        )
    }

    @Test
    fun buildsSpeakPayloadWithTheTextOnlyInTheTask() {
        val payload = ControlJson.speakPayload("action.station_speak", "Добрый вечер")
        assertEquals(
            """{"entry":"action.station_speak","action":"speak","text":"Добрый вечер"}""",
            payload,
        )
        // Normalization happens before this point; the codec must not drop or
        // escape Cyrillic text.
        assertTrue(payload.contains("Добрый вечер"))
    }
}

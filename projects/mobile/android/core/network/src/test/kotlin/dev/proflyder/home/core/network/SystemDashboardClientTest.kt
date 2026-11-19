package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.GuestKind
import dev.proflyder.home.core.model.HealthLevel
import dev.proflyder.home.core.model.Outcome
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

class SystemDashboardClientTest {
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

    private fun client(control: ServerEndpoint? = endpoint()) = HttpSystemDashboardClient(
        Store(ConnectionConfig(endpoint = endpoint(), controlEndpoint = control, bearerToken = "tok-1")),
        OkHttpClient(),
    )

    private fun fixture(name: String): String =
        checkNotNull(javaClass.getResource("/fixtures/system/$name.json")).readText()

    @Test
    fun summaryUsesTheControlHostAndBearer() = runBlocking {
        server.enqueue(MockResponse().setBody(fixture("summary")))
        val summary = (client().summary() as Outcome.Success).value
        val request = server.takeRequest()
        assertEquals("/v1/system/summary", request.path)
        assertEquals("GET", request.method)
        assertEquals("Bearer tok-1", request.getHeader("Authorization"))
        assertEquals(HealthLevel.CRITICAL, summary.status)
        assertEquals(HealthLevel.OK, summary.sections.host)
        assertEquals(HealthLevel.CRITICAL, summary.sections.guests)
        assertEquals(listOf("StationDown", "PinsStale"), summary.alerts.map { it.name })
        assertEquals(HealthLevel.WARNING, summary.alerts[1].severity)
        assertEquals(1_795_100_000L, summary.alerts[0].startedAtUnix)
        assertTrue(summary.alertsAvailable)
    }

    @Test
    fun hostsMapUsageAndGuests() = runBlocking {
        server.enqueue(MockResponse().setBody(fixture("hosts")))
        val hosts = (client().hosts() as Outcome.Success).value
        val host = checkNotNull(hosts.host)
        assertEquals(0.625, checkNotNull(host.memory.ratio), 1e-9)
        assertEquals(0.6, checkNotNull(host.disk.ratio), 1e-9)
        assertEquals(61.0, checkNotNull(host.tempCelsius), 0.0)
        val vpn = hosts.guests.first()
        assertEquals("vpn", vpn.name)
        assertEquals(GuestKind.CT, vpn.kind)
        assertFalse(vpn.running)
        assertEquals(HealthLevel.CRITICAL, vpn.level)
        assertNull(vpn.cpuRatio)
        assertNull(hosts.guests[1].disk.used)
        assertEquals(GuestKind.VM, hosts.guests[1].kind)
    }

    @Test
    fun clusterKeepsServerOrder() = runBlocking {
        server.enqueue(MockResponse().setBody(fixture("k8s")))
        val cluster = (client().cluster() as Outcome.Success).value
        assertEquals(4, cluster.podCount)
        assertEquals(1, cluster.problemCount)
        assertEquals(listOf("currency-bot", "home-assistant", "monitoring"), cluster.namespaces.map { it.name })
        val pod = cluster.namespaces.first().pods.single()
        assertEquals("CrashLoopBackOff", pod.reason)
        assertEquals(0, pod.ready)
        assertEquals(HealthLevel.CRITICAL, pod.level)
    }

    @Test
    fun devicesMapStationAndPhones() = runBlocking {
        server.enqueue(MockResponse().setBody(fixture("devices")))
        val devices = (client().devices() as Outcome.Success).value
        val station = checkNotNull(devices.station)
        assertEquals("v0.5.1020", station.version)
        assertEquals(32.0, checkNotNull(station.latencyMs), 0.0)
        assertNull(station.wifiRssiDbm)
        assertEquals(listOf("pixel"), devices.phones.map { it.id })
    }

    @Test
    fun unknownLevelsAndMissingFieldsAreTolerated() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"status":"purple","extra":1}"""))
        val summary = (client().summary() as Outcome.Success).value
        assertEquals(HealthLevel.UNKNOWN, summary.status)
        assertEquals(HealthLevel.UNKNOWN, summary.sections.k8s)
        assertTrue(summary.alerts.isEmpty())
    }

    @Test
    fun missingControlEndpointFailsWithoutARequest() = runBlocking {
        val result = client(control = null).summary()
        assertTrue(result is Outcome.Failure)
        assertEquals(0, server.requestCount)
    }
}

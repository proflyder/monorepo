package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.HermesAgentStatus
import dev.proflyder.home.core.model.Outcome
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

class HermesPanelClientTest {
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

    private fun client() = HttpHermesPanelClient(
        testHttp(Store(ConnectionConfig(endpoint = endpoint(), controlEndpoint = endpoint(), bearerToken = "tok-1"))),
    )

    private fun enqueue(name: String) {
        val body = checkNotNull(javaClass.getResource("/fixtures/hermes/$name.json")).readText()
        server.enqueue(MockResponse().setBody(body))
    }

    @Test
    fun agentsMapBridgeStatuses() = runBlocking {
        enqueue("agents")
        val agents = (client().agents() as Outcome.Success).value
        val request = server.takeRequest()
        assertEquals("/v1/hermes/agents", request.path)
        assertEquals("Bearer tok-1", request.getHeader("Authorization"))
        assertTrue(agents.available)
        assertEquals(listOf(HermesAgentStatus.RUNNING, HermesAgentStatus.FAILED), agents.agents.map { it.status })
        assertEquals(125L, agents.agents.first().runtimeSeconds)
        assertNull(agents.agents.last().runtimeSeconds)
    }

    @Test
    fun reportsKeepTimesAndText() = runBlocking {
        enqueue("reports")
        val reports = (client().reports() as Outcome.Success).value
        assertEquals("/v1/hermes/reports", server.takeRequest().path)
        assertEquals(1700000030L, reports.reports.single().completedAtUnix)
        assertEquals("Всё в порядке", reports.reports.single().text)
    }

    @Test
    fun voiceKeepsHealthAndRequests() = runBlocking {
        enqueue("voice")
        val voice = (client().voice() as Outcome.Success).value
        assertEquals("/v1/hermes/voice", server.takeRequest().path)
        assertTrue(voice.relayUp && voice.hermesUp)
        assertEquals("1.2", voice.hermesVersion)
        assertEquals(1250L, voice.requests.first().latencyMs)
        assertTrue(voice.requests.last().failed)
        assertNull(voice.requests.last().reply)
    }

    @Test
    fun skillsKeepCategoryOrder() = runBlocking {
        enqueue("skills")
        val skills = (client().skills() as Outcome.Success).value
        assertEquals("/v1/hermes/skills", server.takeRequest().path)
        assertTrue(skills.available)
        assertEquals(listOf("notes", "alarm-helper", "report", "arxiv"), skills.skills.map { it.name })
        assertNull(skills.skills.first().category)
        assertEquals("home", skills.skills[1].category)
    }

    @Test
    fun toolsetsKeepStateAndToolNames() = runBlocking {
        enqueue("toolsets")
        val toolsets = (client().toolsets() as Outcome.Success).value
        assertEquals("/v1/hermes/toolsets", server.takeRequest().path)
        val web = toolsets.toolsets.first()
        assertEquals("Web search", web.label)
        assertTrue(web.enabled && web.configured)
        assertEquals(listOf("web_extract", "web_search"), web.tools)
        assertEquals("memory", toolsets.named("memory")?.label)
    }
}

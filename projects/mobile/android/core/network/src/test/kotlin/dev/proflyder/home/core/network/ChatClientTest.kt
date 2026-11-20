package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatKind
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
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

class ChatClientTest {
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

    private fun client() = HttpChatClient(
        Store(ConnectionConfig(endpoint = endpoint(), controlEndpoint = endpoint(), bearerToken = "tok-1")),
        OkHttpClient(),
    )

    private fun fixture(name: String): String =
        checkNotNull(javaClass.getResource("/fixtures/chat/$name.json")).readText()

    private fun sseFromFixture(): String {
        val items = Json.parseToJsonElement(fixture("events")) as JsonArray
        return items.joinToString(separator = "", prefix = ": keepalive\n\n") { frame(it.jsonObject) }
    }

    private fun frame(item: JsonObject): String =
        "event: ${item.getValue("event").jsonPrimitive.content}\ndata: ${item.getValue("data")}\n\n"

    @Test
    fun pageMapsMessagesQuotesAndCursor() = runBlocking {
        server.enqueue(MockResponse().setBody(fixture("page")))
        val page = (client().page(before = null, limit = 3) as Outcome.Success).value
        val request = server.takeRequest()
        assertEquals("/v1/chat/messages?limit=3", request.path)
        assertEquals("Bearer tok-1", request.getHeader("Authorization"))
        assertTrue(page.hasMore)
        assertEquals(8L, page.cursor)
        assertEquals(listOf(2L, 3L, 4L), page.messages.map { it.seq })
        val question = page.messages[1]
        assertEquals(ChatRole.USER, question.role)
        assertEquals("c-2", question.clientId)
        assertEquals("m_0002", question.replyTo?.id)
        assertEquals(ChatRole.ASSISTANT, question.replyTo?.role)
        assertEquals(ChatStatus.DONE, page.messages[2].status)
        assertEquals(ChatKind.TEXT, page.messages[2].kind)
    }

    @Test
    fun sendPostsTheClientIdAndMapsThePendingReply() = runBlocking {
        server.enqueue(MockResponse().setBody(fixture("send")))
        val sent = (client().send("c-1", "Включи свет", replyTo = null) as Outcome.Success).value
        val request = server.takeRequest()
        assertEquals("POST", request.method)
        assertEquals("/v1/chat/messages", request.path)
        val body = Json.parseToJsonElement(request.body.readUtf8()).jsonObject
        assertEquals("c-1", body.getValue("clientId").jsonPrimitive.content)
        assertEquals("Включи свет", sent.message.text)
        assertEquals(ChatStatus.PENDING, sent.reply.status)
        assertNull(sent.reply.errorKey)
        assertEquals(1_795_129_665_000L, sent.reply.createdAtMillis)
    }

    @Test
    fun errorsCarryTheResourceKey() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(400).setBody(fixture("error")))
        val failure = (client().send("c-1", " ", replyTo = null) as Outcome.Failure).failure
        assertEquals("chat_text_empty", (failure as RemoteFailure.Server).code)
    }

    @Test
    fun retryPostsToTheMessage() = runBlocking {
        server.enqueue(MockResponse().setBody(Json.parseToJsonElement(fixture("send")).jsonObject.getValue("reply").toString()))
        val message = (client().retry("m_0002") as Outcome.Success).value
        assertEquals("/v1/chat/messages/m_0002/retry", server.takeRequest().path)
        assertEquals("m_0002", message.id)
    }

    @Test
    fun eventsParseTheServerStream() = runBlocking {
        server.enqueue(MockResponse().setHeader("Content-Type", "text/event-stream").setBody(sseFromFixture()))
        val events = client().events(after = 0).toList()
        assertEquals("/v1/chat/events?after=0", server.takeRequest().path)
        assertEquals(6, events.size)
        val deltas = events.filterIsInstance<ChatEvent.Delta>()
        assertEquals(listOf(0, 5), deltas.map { it.offset })
        assertEquals("Свет включён.", deltas.joinToString("") { it.text })
        val last = (events.last() as ChatEvent.Upsert).message
        assertEquals(ChatStatus.DONE, last.status)
        assertEquals("Свет **включён**.", last.text)
        assertFalse(events.first() is ChatEvent.Delta)
    }

    @Test
    fun unauthorizedEventsFail() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(401))
        val result = runCatching { client().events(after = 0).toList() }
        assertTrue(result.exceptionOrNull() is ChatUnauthorizedException)
    }
}

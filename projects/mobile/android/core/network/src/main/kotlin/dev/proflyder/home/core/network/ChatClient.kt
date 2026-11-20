package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatKind
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatSent
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.core.model.Outcome
import java.io.IOException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import okhttp3.Interceptor
import okhttp3.OkHttpClient
import okhttp3.Request
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

interface ChatClient {
    suspend fun page(before: Long?, limit: Int): Outcome<ChatPage>

    suspend fun send(clientId: String, text: String, replyTo: String?): Outcome<ChatSent>

    suspend fun retry(id: String): Outcome<ChatMessage>

    fun events(after: Long): Flow<ChatEvent>
}

@Serializable
internal data class ChatQuoteDto(
    val id: String = "",
    val role: String = "",
    val text: String = "",
)

@Serializable
internal data class ChatMessageDto(
    val id: String = "",
    val seq: Long = 0,
    val version: Long = 0,
    val clientId: String? = null,
    val role: String = "",
    val kind: String = "",
    val text: String = "",
    val replyTo: ChatQuoteDto? = null,
    val status: String = "",
    val error: String? = null,
    val createdAt: Long = 0,
    val updatedAt: Long = 0,
)

@Serializable
internal data class ChatPageDto(
    val messages: List<ChatMessageDto> = emptyList(),
    val hasMore: Boolean = false,
    val cursor: Long = 0,
)

@Serializable
internal data class ChatSendRequestDto(
    val clientId: String,
    val text: String,
    val replyTo: String? = null,
)

@Serializable
internal data class ChatSendResponseDto(
    val message: ChatMessageDto = ChatMessageDto(),
    val reply: ChatMessageDto = ChatMessageDto(),
)

@Serializable
internal data class ChatDeltaDto(
    val id: String = "",
    val offset: Int = 0,
    val text: String = "",
)

internal interface ChatApi {
    @GET("v1/chat/messages")
    fun page(@Query("before") before: Long?, @Query("limit") limit: Int): Outcome<ChatPageDto>

    @POST("v1/chat/messages")
    fun send(@Body body: ChatSendRequestDto): Outcome<ChatSendResponseDto>

    @POST("v1/chat/messages/{id}/retry")
    fun retry(@Path("id") id: String, @Body body: EmptyBody): Outcome<ChatMessageDto>
}

internal fun String.toChatRole(): ChatRole = when (this) {
    "user" -> ChatRole.USER
    "assistant" -> ChatRole.ASSISTANT
    else -> ChatRole.SYSTEM
}

internal fun String.toChatKind(): ChatKind = when (this) {
    "alert" -> ChatKind.ALERT
    else -> ChatKind.TEXT
}

internal fun String.toChatStatus(): ChatStatus = when (this) {
    "pending" -> ChatStatus.PENDING
    "streaming" -> ChatStatus.STREAMING
    "failed" -> ChatStatus.FAILED
    else -> ChatStatus.DONE
}

internal fun ChatMessageDto.toDomain(): ChatMessage = ChatMessage(
    id = id,
    seq = seq,
    version = version,
    clientId = clientId,
    role = role.toChatRole(),
    kind = kind.toChatKind(),
    text = text,
    replyTo = replyTo?.let { ChatQuote(it.id, it.role.toChatRole(), it.text) },
    status = status.toChatStatus(),
    errorKey = error,
    createdAtMillis = createdAt,
    updatedAtMillis = updatedAt,
)

internal fun ChatPageDto.toDomain(): ChatPage =
    ChatPage(messages.map { it.toDomain() }, hasMore, cursor)

internal class ChatSseReader {
    private var name = ""
    private val data = StringBuilder()

    fun line(line: String): ChatEvent? = when {
        line.isEmpty() -> flush()
        line.startsWith(":") -> null
        line.startsWith("event:") -> null.also { name = line.removePrefix("event:").trim() }
        line.startsWith("data:") -> null.also { appendData(line.removePrefix("data:")) }
        else -> null
    }

    private fun appendData(value: String) {
        if (data.isNotEmpty()) data.append('\n')
        data.append(value.removePrefix(" "))
    }

    private fun flush(): ChatEvent? {
        val event = decode(name, data.toString())
        name = ""
        data.setLength(0)
        return event
    }

    private fun decode(event: String, payload: String): ChatEvent? = when (event) {
        "message" -> ChatEvent.Upsert(WireJson.decodeFromString(ChatMessageDto.serializer(), payload).toDomain())
        "delta" -> WireJson.decodeFromString(ChatDeltaDto.serializer(), payload).toEvent()
        else -> null
    }

    private fun ChatDeltaDto.toEvent(): ChatEvent = ChatEvent.Delta(id, offset, text)
}

class HttpChatClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    interceptors: List<Interceptor> = emptyList(),
    networkWaiter: NetworkWaiter = NetworkWaiter.Online,
) : ChatClient {
    private val api: ChatApi = HomeRetrofit.create(
        ChatApi::class.java,
        settings,
        baseClient,
        Target.CONTROL,
        interceptors,
        networkWaiter,
    )

    private val streamClient: OkHttpClient = baseClient.newBuilder()
        .followRedirects(false)
        .followSslRedirects(false)
        .retryOnConnectionFailure(false)
        .addInterceptor(ConnectionInterceptor(settings, Target.CONTROL))
        .apply { interceptors.forEach(::addInterceptor) }
        .build()

    override suspend fun page(before: Long?, limit: Int): Outcome<ChatPage> =
        withContext(ioDispatcher) { api.page(before, limit).mapResult { it.toDomain() } }

    override suspend fun send(clientId: String, text: String, replyTo: String?): Outcome<ChatSent> =
        withContext(ioDispatcher) {
            api.send(ChatSendRequestDto(clientId, text, replyTo))
                .mapResult { ChatSent(it.message.toDomain(), it.reply.toDomain()) }
        }

    override suspend fun retry(id: String): Outcome<ChatMessage> =
        withContext(ioDispatcher) { api.retry(id, EmptyBody).mapResult { it.toDomain() } }

    override fun events(after: Long): Flow<ChatEvent> = flow {
        val call = streamClient.newCall(eventsRequest(after))
        val cancellation = currentCoroutineContext()[Job]?.invokeOnCompletion { call.cancel() }
        runCatching { call.execute().use { response -> readEvents(response) { emit(it) } } }
            .also { cancellation?.dispose() }
            .getOrThrow()
    }.flowOn(ioDispatcher)

    private fun eventsRequest(after: Long): Request = Request.Builder()
        .url("${PLACEHOLDER_BASE_URL}v1/chat/events?after=$after")
        .header(STREAM_REQUEST_HEADER, "1")
        .header("Accept", EVENT_STREAM)
        .get()
        .build()

    private suspend fun readEvents(response: okhttp3.Response, onEvent: suspend (ChatEvent) -> Unit) {
        checkResponse(response)
        val reader = ChatSseReader()
        val source = response.body.source()
        while (true) {
            val line = source.readUtf8Line() ?: return
            reader.line(line)?.let { onEvent(it) }
        }
    }

    private fun checkResponse(response: okhttp3.Response) = when {
        response.code == 401 || response.code == 403 -> throw ChatUnauthorizedException()
        !response.isSuccessful -> throw IOException("HTTP ${response.code}")
        else -> Unit
    }

    private companion object {
        const val PLACEHOLDER_BASE_URL = "http://localhost/"
        const val EVENT_STREAM = "text/event-stream"
    }
}

class ChatUnauthorizedException : IOException("chat events unauthorized")

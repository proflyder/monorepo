package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ChatAlert
import dev.proflyder.home.core.model.ChatAttachment
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatKind
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatSent
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.core.model.Outcome
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.ResponseBody
import okio.BufferedSource
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

@Serializable
internal data class ChatQuoteDto(
    val id: String = "",
    val role: String = "",
    val text: String = "",
)

@Serializable
internal data class ChatAttachmentDto(
    val id: String = "",
    val kind: String = "",
    val mime: String = "",
    val size: Long = 0,
    val name: String = "",
    val durationMs: Long? = null,
)

@Serializable
internal data class ChatAlertDto(
    val fingerprint: String = "",
    val name: String = "",
    val severity: String = "",
    val status: String = "",
    val summary: String = "",
    val description: String = "",
    val namespace: String = "",
    @SerialName("object") val target: String = "",
    val startsAt: Long = 0,
    val endsAt: Long? = null,
    val runbookUrl: String = "",
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
    val attachments: List<ChatAttachmentDto> = emptyList(),
    val transcript: String? = null,
    val alert: ChatAlertDto? = null,
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
    val attachments: List<String> = emptyList(),
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
    @Idempotent
    @GET("v1/chat/messages")
    suspend fun page(@Query("before") before: Long?, @Query("limit") limit: Int): Outcome<ChatPageDto>

    @POST("v1/chat/messages")
    suspend fun send(@Body body: ChatSendRequestDto): Outcome<ChatSendResponseDto>

    @POST("v1/chat/messages/{id}/retry")
    suspend fun retry(@Path("id") id: String, @Body body: EmptyBody): Outcome<ChatMessageDto>

    @POST("v1/chat/attachments")
    suspend fun upload(@Query("name") name: String, @Body body: RequestBody): Outcome<ChatAttachmentDto>

    @Idempotent
    @GET("v1/chat/attachments/{id}")
    suspend fun download(@Path("id") id: String): Outcome<ResponseBody>
}

internal fun String.toChatRole(): ChatRole = when (this) {
    "user" -> ChatRole.USER
    "assistant" -> ChatRole.ASSISTANT
    else -> ChatRole.SYSTEM
}

internal fun String.toChatKind(): ChatKind = when (this) {
    "alert" -> ChatKind.ALERT
    "voice" -> ChatKind.VOICE
    "image" -> ChatKind.IMAGE
    "file" -> ChatKind.FILE
    else -> ChatKind.TEXT
}

internal fun ChatAttachmentDto.toDomain(): ChatAttachment = ChatAttachment(
    id = id,
    kind = when (kind) {
        "voice" -> ChatAttachmentKind.VOICE
        "image" -> ChatAttachmentKind.IMAGE
        else -> ChatAttachmentKind.FILE
    },
    mime = mime,
    size = size,
    name = name,
    durationMs = durationMs,
)

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
    attachments = attachments.map { it.toDomain() },
    transcript = transcript,
    alert = alert?.let {
        ChatAlert(
            fingerprint = it.fingerprint,
            name = it.name,
            severity = it.severity,
            resolved = it.status == "resolved",
            summary = it.summary,
            description = it.description,
            namespace = it.namespace,
            target = it.target,
            startsAtMillis = it.startsAt,
            endsAtMillis = it.endsAt,
            runbookUrl = it.runbookUrl,
        )
    },
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
        "message" -> ChatEvent.Upsert(NetworkJson.decodeFromString(ChatMessageDto.serializer(), payload).toDomain())
        "delta" -> NetworkJson.decodeFromString(ChatDeltaDto.serializer(), payload).toEvent()
        else -> null
    }

    private fun ChatDeltaDto.toEvent(): ChatEvent = ChatEvent.Delta(id, offset, text)
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpChatClient(private val http: HomeHttp) : ChatClient {
    private val api: ChatApi by lazy { http.rest(ChatApi::class, Target.Control) }

    override suspend fun page(before: Long?, limit: Int): Outcome<ChatPage> =
        api.page(before, limit).mapResult { it.toDomain() }

    override suspend fun send(
        clientId: String,
        text: String,
        replyTo: String?,
        attachments: List<String>,
    ): Outcome<ChatSent> = api.send(ChatSendRequestDto(clientId, text, replyTo, attachments))
        .mapResult { ChatSent(it.message.toDomain(), it.reply.toDomain()) }

    override suspend fun upload(bytes: ByteArray, mime: String, name: String): Outcome<ChatAttachment> =
        api.upload(name, bytes.toRequestBody(mime.toMediaType())).mapResult { it.toDomain() }

    override suspend fun download(id: String): Outcome<ByteArray> =
        api.download(id).mapResult { body -> body.use { it.bytes() } }

    override suspend fun retry(id: String): Outcome<ChatMessage> =
        api.retry(id, EmptyBody).mapResult { it.toDomain() }

    override fun events(after: Long): Flow<ChatEvent> =
        http.stream(Target.Control, StreamRequest("v1/chat/events?after=$after", EVENT_STREAM), ChatSseDecoder)

    private companion object {
        const val EVENT_STREAM = "text/event-stream"
    }
}

internal object ChatSseDecoder : StreamDecoder<ChatEvent> {
    override suspend fun decode(source: BufferedSource, emit: suspend (ChatEvent) -> Unit) {
        val reader = ChatSseReader()
        generateSequence { source.readUtf8Line() }
            .forEach { line -> reader.line(line)?.let { emit(it) } }
    }
}

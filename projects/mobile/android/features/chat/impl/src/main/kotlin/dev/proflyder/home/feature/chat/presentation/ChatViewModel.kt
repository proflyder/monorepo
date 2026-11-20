package dev.proflyder.home.feature.chat.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import android.net.Uri
import dev.proflyder.home.feature.chat.data.ChatMedia
import dev.proflyder.home.feature.chat.data.ChatRepository
import dev.proflyder.home.feature.chat.data.LocalAttachment
import dev.proflyder.home.feature.chat.data.Prepared
import java.io.File
import dev.proflyder.home.feature.chat.data.ChatTimeline
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import java.time.ZoneId
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

internal data class ChatUiState(
    val items: List<ChatItem> = emptyList(),
    val replyTo: ChatQuote? = null,
    val pending: List<LocalAttachment> = emptyList(),
    val loaded: Boolean = false,
    val failure: RemoteFailure? = null,
    val hasMore: Boolean = false,
)

internal class ChatViewModel(
    private val repository: ChatRepository,
    private val media: ChatMedia,
    private val zone: () -> ZoneId = ZoneId::systemDefault,
) : ViewModel() {
    private val reply = MutableStateFlow<ChatQuote?>(null)
    private val pending = MutableStateFlow<List<LocalAttachment>>(emptyList())
    private val playback = MutableStateFlow<VoicePlayback?>(null)
    private val failures = MutableSharedFlow<RemoteFailure>(extraBufferCapacity = 1)
    private val rejections = MutableSharedFlow<String>(extraBufferCapacity = 1)

    val state: StateFlow<ChatUiState> = combine(repository.timeline, reply, pending, playback, ::toState)
        .stateIn(viewModelScope, SharingStarted.Eagerly, ChatUiState())

    val errors = failures.asSharedFlow()

    val rejected = rejections.asSharedFlow()

    private fun toState(
        timeline: ChatTimeline,
        quote: ChatQuote?,
        attachments: List<LocalAttachment>,
        voice: VoicePlayback?,
    ): ChatUiState = ChatUiState(
        items = ChatItems.build(timeline, zone(), voice),
        replyTo = quote,
        pending = attachments,
        loaded = timeline.loaded,
        failure = timeline.failure,
        hasMore = timeline.hasMore,
    )

    suspend fun runLive() = repository.runLive()

    fun refresh() {
        viewModelScope.launch { report(repository.refresh()) }
    }

    fun loadOlder() {
        viewModelScope.launch { repository.loadOlder() }
    }

    fun send(text: String): Boolean {
        val attachments = pending.value
        if (text.isBlank() && attachments.isEmpty()) return false
        repository.send(text, reply.value, attachments)
        reply.value = null
        pending.value = emptyList()
        return true
    }

    fun sendVoice(voice: LocalAttachment) {
        repository.send("", reply.value, listOf(voice))
        reply.value = null
    }

    fun newVoiceFile(): File = media.newFile(VOICE_EXTENSION)

    fun attachImage(uri: Uri) {
        viewModelScope.launch { accept(media.prepareImage(uri)) }
    }

    fun attachFile(uri: Uri) {
        viewModelScope.launch { accept(media.prepareFile(uri)) }
    }

    private fun accept(prepared: Prepared) {
        when (prepared) {
            is Prepared.Ready -> pending.value = (pending.value + prepared.attachment).takeLast(MAX_PENDING)
            is Prepared.Rejected -> rejections.tryEmit(prepared.key)
        }
    }

    fun removePending(path: String) {
        pending.value = pending.value.filterNot { it.path == path }
    }

    suspend fun mediaFile(item: BubbleMedia): File? =
        item.path?.let(::File)?.takeIf { it.isFile } ?: item.id?.let { media.fileFor(it) }

    fun onPlayback(state: VoicePlayback?) {
        playback.value = state
    }

    fun replyTo(bubble: ChatItem.Bubble) {
        val id = bubble.messageId ?: return
        reply.value = ChatQuote(id, if (bubble.mine) ChatRole.USER else ChatRole.ASSISTANT, bubble.text)
    }

    fun cancelReply() {
        reply.value = null
    }

    fun retry(bubble: ChatItem.Bubble) {
        val clientId = bubble.clientId
        when {
            bubble.messageId == null && clientId != null -> repository.retryOutgoing(clientId)
            bubble.messageId != null && !bubble.mine -> retryReply(bubble.messageId)
            else -> Unit
        }
    }

    fun discard(bubble: ChatItem.Bubble) {
        val clientId = bubble.clientId ?: return
        if (bubble.messageId == null) repository.discardOutgoing(clientId)
    }

    private fun retryReply(id: String) {
        viewModelScope.launch { report(repository.retryReply(id)) }
    }

    private fun report(outcome: Outcome<Unit>) {
        if (outcome is Outcome.Failure) failures.tryEmit(outcome.failure)
    }
}

private const val VOICE_EXTENSION = "wav"
private const val MAX_PENDING = 4

@ContributesIntoSet(AppScope::class)
@Inject
class ChatViewModelFactoryContributor(
    private val repository: ChatRepository,
    private val media: ChatMedia,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ChatViewModel::class.java

    override fun createFactory(): ViewModelProvider.Factory = SimpleViewModelFactory { ChatViewModel(repository, media) }
}

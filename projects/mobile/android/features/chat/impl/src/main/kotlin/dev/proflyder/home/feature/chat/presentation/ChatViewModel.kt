package dev.proflyder.home.feature.chat.presentation

import android.graphics.Bitmap
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.StateViewModel
import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.model.ChatSession
import dev.proflyder.home.core.navigation.destination
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.feature.chat.ChatConversationDestination
import dev.proflyder.home.feature.chat.ChatRoute
import dev.proflyder.home.feature.chat.data.ChatSessionsRepository
import dev.proflyder.home.feature.chat.data.SessionAction
import dev.proflyder.home.feature.chat.data.SessionActions
import dev.proflyder.home.feature.chat.data.SessionsSnapshot
import dev.proflyder.home.feature.chat.data.AttachmentSource
import dev.proflyder.home.feature.chat.data.AudioVoiceRecorder
import dev.proflyder.home.feature.chat.data.ChatComponents
import dev.proflyder.home.feature.chat.data.ChatMedia
import dev.proflyder.home.feature.chat.data.ChatRepository
import dev.proflyder.home.feature.chat.data.ChatTimeline
import dev.proflyder.home.feature.chat.data.LIVE_STOP_MILLIS
import dev.proflyder.home.feature.chat.data.LocalAttachment
import dev.proflyder.home.feature.chat.data.MediaRef
import dev.proflyder.home.feature.chat.data.Prepared
import dev.proflyder.home.feature.chat.data.VoiceCapture
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.time.ZoneId
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingCommand
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class ChatUiState(
    val items: List<ChatItem> = emptyList(),
    val replyTo: ChatQuote? = null,
    val pending: List<LocalAttachment> = emptyList(),
    val draft: String = "",
    val recording: RecordingState = RecordingState.Idle,
    val loaded: Boolean = false,
    val failure: RemoteFailure? = null,
    val hasMore: Boolean = false,
    val session: ChatSession? = null,
    val activeTool: String? = null,
    val stoppableReplyId: String? = null,
) {
    val canSend: Boolean get() = draft.isNotBlank() || pending.isNotEmpty()
}

internal sealed interface ChatIntent {
    data class InputChanged(val text: String) : ChatIntent
    data object Send : ChatIntent
    data object Refresh : ChatIntent
    data object LoadOlder : ChatIntent
    data class ReplyTo(val bubble: ChatItem.Bubble) : ChatIntent
    data object CancelReply : ChatIntent
    data class Retry(val bubble: ChatItem.Bubble) : ChatIntent
    data class Discard(val bubble: ChatItem.Bubble) : ChatIntent
    data class AttachImage(val source: AttachmentSource) : ChatIntent
    data class AttachFile(val source: AttachmentSource) : ChatIntent
    data class RemovePending(val path: String) : ChatIntent
    data object StartRecording : ChatIntent
    data class FinishRecording(val send: Boolean) : ChatIntent
    data class ToggleVoice(val media: BubbleMedia) : ChatIntent
    data object Stop : ChatIntent
    data class Act(val action: SessionAction) : ChatIntent
}

internal sealed interface ChatEvent {
    data class Failure(val failure: RemoteFailure) : ChatEvent
    data class Rejected(val key: String) : ChatEvent
    data object Closed : ChatEvent
}

internal class ConversationSources(
    val sessionId: String,
    val repository: ChatRepository,
    val sessions: ChatSessionsRepository,
    val actions: SessionActions,
)

internal class ChatViewModel(
    private val sources: ConversationSources,
    private val media: ChatMedia,
    capture: VoiceCapture,
    private val player: VoicePlayer,
    private val zone: () -> ZoneId = ZoneId::systemDefault,
) : StateViewModel<ChatUiState, ChatIntent, ChatEvent>(ChatUiState()) {
    private val recording = VoiceRecording(capture, viewModelScope)
    private val repository: ChatRepository = sources.repository
    val sessionId: String get() = sources.sessionId

    init {
        whileObserved(::timelineState)
        recording.state.onEach(::onRecording).launchIn(viewModelScope)
        sources.sessions.sessions.onEach(::onSessions).launchIn(viewModelScope)
    }

    override fun handle(intent: ChatIntent) {
        when (intent) {
            is ChatIntent.InputChanged -> updateState { copy(draft = intent.text) }
            ChatIntent.Send -> send()
            ChatIntent.Refresh -> launch { report(repository.refresh()) }
            ChatIntent.LoadOlder -> if (currentState.hasMore) launch { repository.loadOlder() }
            is ChatIntent.ReplyTo -> replyTo(intent.bubble)
            ChatIntent.CancelReply -> updateState { copy(replyTo = null) }
            is ChatIntent.Retry -> retry(intent.bubble)
            is ChatIntent.Discard -> discard(intent.bubble)
            ChatIntent.Stop -> stop()
            is ChatIntent.Act -> act(intent.action)
            else -> handleMedia(intent)
        }
    }

    suspend fun thumbnail(item: BubbleMedia, width: Int): Bitmap? = media.thumbnail(item.ref, width)

    suspend fun waveLevels(item: BubbleMedia): FloatArray? = media.waveLevels(item.ref, VoiceWaveView.BARS)

    private fun handleMedia(intent: ChatIntent) {
        when (intent) {
            is ChatIntent.AttachImage -> launch { accept(media.prepareImage(intent.source)) }
            is ChatIntent.AttachFile -> launch { accept(media.prepareFile(intent.source)) }
            is ChatIntent.RemovePending -> updateState { copy(pending = pending.filterNot { it.path == intent.path }) }
            ChatIntent.StartRecording -> if (recording.start()) player.release()
            is ChatIntent.FinishRecording -> finishRecording(intent.send)
            is ChatIntent.ToggleVoice -> launch { media.localPath(intent.media.ref)?.let { player.toggle(viewModelScope, intent.media.key, it) } }
            else -> Unit
        }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun whileObserved(source: () -> Flow<Unit>) {
        SharingStarted.WhileSubscribed(LIVE_STOP_MILLIS).command(mutableState.subscriptionCount)
            .distinctUntilChanged()
            .flatMapLatest { command -> if (command == SharingCommand.START) source() else emptyFlow() }
            .launchIn(viewModelScope)
    }

    private fun timelineState(): Flow<Unit> = combine(repository.timeline, player.playback, ::onTimeline)

    private fun onTimeline(timeline: ChatTimeline, voice: VoicePlayback?) = updateState {
        copy(
            items = ChatItems.build(timeline, zone(), voice),
            loaded = timeline.loaded,
            failure = timeline.failure,
            hasMore = timeline.hasMore,
            activeTool = timeline.activeTool,
            stoppableReplyId = timeline.streamingReplyId,
        )
    }

    private fun onSessions(snapshot: SessionsSnapshot) {
        snapshot.find(sources.sessionId)?.let { updateState { copy(session = it) } }
    }

    private fun stop() {
        val id = currentState.stoppableReplyId ?: return
        launch { report(repository.stop(id)) }
    }

    private fun act(action: SessionAction) {
        launch { acted(action, sources.actions.perform(action)) }
    }

    private fun acted(action: SessionAction, outcome: Outcome<Unit>) {
        report(outcome)
        if (outcome is Outcome.Success && action is SessionAction.Delete) emit(ChatEvent.Closed)
    }

    private fun onRecording(state: RecordingState) {
        updateState { copy(recording = state) }
        if (state is RecordingState.Active && state.elapsedMillis >= VoiceRecording.MAX_MILLIS) finishRecording(send = true)
    }

    private fun send() {
        val state = currentState
        if (!state.canSend) return
        repository.send(state.draft, state.replyTo, state.pending)
        updateState { copy(draft = "", replyTo = null, pending = emptyList()) }
    }

    private fun finishRecording(send: Boolean) {
        if (!send) return recording.cancel()
        launch { recording.stop()?.let(::sendVoice) }
    }

    private fun sendVoice(voice: LocalAttachment) {
        repository.send("", currentState.replyTo, listOf(voice))
        updateState { copy(replyTo = null) }
    }

    private fun accept(prepared: Prepared) {
        when (prepared) {
            is Prepared.Ready -> updateState { copy(pending = (pending + prepared.attachment).takeLast(MAX_PENDING)) }
            is Prepared.Rejected -> emit(ChatEvent.Rejected(prepared.key))
        }
    }

    private fun replyTo(bubble: ChatItem.Bubble) {
        val id = bubble.messageId ?: return
        updateState { copy(replyTo = ChatQuote(id, if (bubble.mine) ChatRole.USER else ChatRole.ASSISTANT, bubble.text)) }
    }

    private fun retry(bubble: ChatItem.Bubble) {
        val clientId = bubble.clientId
        when {
            bubble.messageId == null && clientId != null -> repository.retryOutgoing(clientId)
            bubble.messageId != null && !bubble.mine -> launch { report(repository.retryReply(bubble.messageId)) }
            else -> Unit
        }
    }

    private fun discard(bubble: ChatItem.Bubble) {
        val clientId = bubble.clientId ?: return
        if (bubble.messageId == null) repository.discardOutgoing(clientId)
    }

    private fun report(outcome: Outcome<Unit>) {
        val failure = (outcome as? Outcome.Failure)?.failure ?: return
        if (currentState.loaded || currentState.items.isNotEmpty()) emit(ChatEvent.Failure(failure))
    }

    override fun onCleared() {
        recording.cancel()
        player.release()
    }

    private companion object {
        const val MAX_PENDING = 4
    }
}

internal val BubbleMedia.ref: MediaRef get() = MediaRef(id, path)

@ContributesIntoMap(AppScope::class, binding = binding<SavedStateViewModelFactory>())
@ClassKey(ChatViewModel::class)
@Inject
class ChatViewModelSource internal constructor(
    private val components: ChatComponents,
) : SavedStateViewModelFactory {
    override fun create(savedState: SavedStateHandle): ViewModel {
        val sessionId = savedState.destination(ChatConversationDestination)?.sessionId ?: ChatRoute.LEGACY_SESSION_ID
        val sources = ConversationSources(sessionId, components.repository(sessionId), components.sessions, components.sessionActions)
        return ChatViewModel(sources, components.media, AudioVoiceRecorder(components.media::newVoiceFile), ChatVoicePlayer())
    }
}

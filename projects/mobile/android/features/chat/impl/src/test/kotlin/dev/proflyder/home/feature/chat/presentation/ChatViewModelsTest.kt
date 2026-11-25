package dev.proflyder.home.feature.chat.presentation

import android.graphics.Bitmap
import dev.proflyder.home.core.model.ChatEvent as WireEvent
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatSessionPatch
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.chat.ChatFixtures
import dev.proflyder.home.feature.chat.data.AttachmentSource
import dev.proflyder.home.feature.chat.data.ChatMedia
import dev.proflyder.home.feature.chat.data.FakeChatClient
import dev.proflyder.home.feature.chat.data.LiveChatRepository
import dev.proflyder.home.feature.chat.data.LiveChatSessionsRepository
import dev.proflyder.home.feature.chat.data.LocalAttachment
import dev.proflyder.home.feature.chat.data.MediaRef
import dev.proflyder.home.feature.chat.data.MemoryOutboxStore
import dev.proflyder.home.feature.chat.data.Prepared
import dev.proflyder.home.feature.chat.data.SessionAction
import dev.proflyder.home.feature.chat.data.SessionActions
import dev.proflyder.home.feature.chat.data.VoiceCapture
import dev.proflyder.home.feature.chat.presentation.sessions.ChatSessionsEvent
import dev.proflyder.home.feature.chat.presentation.sessions.ChatSessionsIntent
import dev.proflyder.home.feature.chat.presentation.sessions.ChatSessionsViewModel
import dev.proflyder.home.feature.chat.presentation.sessions.SessionRow
import dev.proflyder.home.feature.chat.presentation.sessions.SessionRows
import dev.proflyder.home.feature.chat.presentation.sessions.listStatus
import java.io.File
import java.time.ZoneOffset
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ChatViewModelsTest {
    private val client = FakeChatClient().apply {
        sessions = ChatFixtures.sessions.toMutableList()
        events = flow { awaitCancellation() }
    }
    private val forgotten = mutableListOf<String>()

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun TestScope.lifetime(): CoroutineScope =
        CoroutineScope(backgroundScope.coroutineContext + UnconfinedTestDispatcher(testScheduler))

    private fun TestScope.sessionsRepository() = LiveChatSessionsRepository(client, lifetime())

    private fun TestScope.listModel(archived: Boolean = false): ChatSessionsViewModel {
        val repository = sessionsRepository()
        return ChatSessionsViewModel(repository, SessionActions(repository) { forgotten += it }, archived)
    }

    @Test
    fun theListShowsActiveSessionsPinnedFirstThenNewestWithAnArchiveRow() = runTest {
        val model = listModel()
        advanceUntilIdle()
        val state = model.state.value
        assertEquals(listOf("home-app", "s_trip", "s_k3s", "s_shop", "s_new"), state.sessions.map { it.id })
        assertEquals(1, state.archivedCount)
        val rows = SessionRows.build(state)
        assertEquals(SessionRow.Archive(1), rows.first())
        assertNull(listStatus(state).emptyText)
    }

    @Test
    fun searchFiltersByTitleAndPreviewAndHidesTheArchiveRow() = runTest {
        val model = listModel()
        advanceUntilIdle()
        model.dispatch(ChatSessionsIntent.Search("гор"))
        assertEquals(listOf("s_trip"), model.state.value.visible.map { it.id })
        assertTrue(SessionRows.build(model.state.value).none { it is SessionRow.Archive })
        model.dispatch(ChatSessionsIntent.Search("нет такого"))
        assertEquals(dev.proflyder.home.feature.chat.impl.R.string.chat_session_nothing_found, listStatus(model.state.value).emptyText)
    }

    @Test
    fun theArchiveScreenListsOnlyArchivedSessions() = runTest {
        val model = listModel(archived = true)
        advanceUntilIdle()
        assertEquals(listOf("s_old"), model.state.value.sessions.map { it.id })
        assertTrue(SessionRows.build(model.state.value).all { it is SessionRow.Item })
    }

    @Test
    fun creatingASessionOpensIt() = runTest {
        val model = listModel()
        advanceUntilIdle()
        val opened = backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { assertEquals(ChatSessionsEvent.Open("s_new"), model.events.first()) }
        model.dispatch(ChatSessionsIntent.Create)
        advanceUntilIdle()
        assertTrue(opened.isCompleted)
        assertEquals(false, model.state.value.creating)
    }

    @Test
    fun sessionActionsPatchTheServerAndReorderTheList() = runTest {
        val model = listModel()
        advanceUntilIdle()
        model.dispatch(ChatSessionsIntent.Act(SessionAction.Pin("s_shop", pinned = true)))
        model.dispatch(ChatSessionsIntent.Act(SessionAction.Rename("s_new", "  Идеи  ")))
        model.dispatch(ChatSessionsIntent.Act(SessionAction.Archive("s_trip", archived = true)))
        model.dispatch(ChatSessionsIntent.Act(SessionAction.Delete("s_k3s")))
        advanceUntilIdle()
        assertEquals(
            listOf(
                "s_shop" to ChatSessionPatch(pinned = true),
                "s_new" to ChatSessionPatch(title = "Идеи"),
                "s_trip" to ChatSessionPatch(archived = true),
            ),
            client.patches,
        )
        assertEquals(listOf("s_k3s"), client.deleted)
        assertEquals(listOf("s_k3s"), forgotten)
        val state = model.state.value
        assertEquals(listOf("home-app", "s_shop", "s_new"), state.sessions.map { it.id })
        assertEquals("Идеи", state.sessions.last().title)
        assertEquals(2, state.archivedCount)
    }

    @Test
    fun aFailedLoadIsShownUntilSessionsArrive() = runTest {
        client.sessionFailure = RemoteFailure.Timeout
        val model = listModel()
        advanceUntilIdle()
        assertEquals(RemoteFailure.Timeout, listStatus(model.state.value).failure)
    }

    private fun TestScope.conversation(sessionId: String = "s_k3s"): ChatViewModel {
        val sessions = sessionsRepository()
        val repository = LiveChatRepository(sessionId, client, MemoryOutboxStore(), lifetime(), OpenGate)
        val sources = ConversationSources(sessionId, repository, sessions, SessionActions(sessions) { forgotten += it })
        return ChatViewModel(sources, NoMedia, SilentCapture, SilentPlayer, zone = { ZoneOffset.ofHours(5) })
    }

    @Test
    fun theConversationShowsItsSessionTitleToolAndStopsTheReply() = runTest {
        val streaming = ChatFixtures.message("a20", 20, ChatRole.ASSISTANT, "Смотрю", status = ChatStatus.STREAMING)
        client.page = dev.proflyder.home.core.model.Outcome.Success(dev.proflyder.home.core.model.ChatPage(listOf(streaming), false, 20))
        client.events = flow {
            emit(WireEvent.Tool("a20", "kubernetes_logs", done = false))
            awaitCancellation()
        }
        val model = conversation()
        val watcher = backgroundScope.launch { model.state.collect { } }
        advanceUntilIdle()
        val state = model.state.value
        assertEquals("Почему падает scheduler", state.session?.title)
        assertEquals("kubernetes_logs", state.activeTool)
        assertEquals("a20", state.stoppableReplyId)
        assertEquals(listOf("s_k3s"), client.pagedSessions.distinct())
        model.dispatch(ChatIntent.Stop)
        advanceUntilIdle()
        assertEquals(listOf("a20"), client.stopped)
        assertNull(model.state.value.stoppableReplyId)
        watcher.cancel()
    }

    @Test
    fun deletingTheOpenSessionClosesTheConversation() = runTest {
        val model = conversation()
        val closed = backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { assertEquals(ChatEvent.Closed, model.events.first()) }
        model.dispatch(ChatIntent.Act(SessionAction.Delete("s_k3s")))
        advanceUntilIdle()
        assertTrue(closed.isCompleted)
        assertEquals(listOf("s_k3s"), client.deleted)
    }

    private object OpenGate : dev.proflyder.home.core.network.RouteGate {
        override suspend fun await() = Unit
    }

    private object NoMedia : ChatMedia {
        override fun newVoiceFile(): File = File.createTempFile("voice", ".wav")

        override suspend fun prepareImage(source: AttachmentSource): Prepared = Prepared.Rejected("chat_attachment_unsupported")

        override suspend fun prepareFile(source: AttachmentSource): Prepared = Prepared.Rejected("chat_attachment_unsupported")

        override suspend fun thumbnail(media: MediaRef, width: Int): Bitmap? = null

        override suspend fun waveLevels(media: MediaRef, bars: Int): FloatArray? = null

        override suspend fun localPath(media: MediaRef): String? = null
    }

    private object SilentCapture : VoiceCapture {
        override fun start(scope: CoroutineScope): Boolean = false

        override fun elapsedMillis(): Long = 0

        override suspend fun stop(): LocalAttachment? = null

        override fun cancel() = Unit
    }

    private object SilentPlayer : VoicePlayer {
        override val playback: StateFlow<VoicePlayback?> = MutableStateFlow(null)

        override fun toggle(scope: CoroutineScope, mediaKey: String, path: String) = Unit

        override fun release() = Unit
    }
}

package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.network.RouteGate
import dev.proflyder.home.core.runtime.Backoff
import java.io.File
import java.io.IOException
import kotlin.time.Duration.Companion.milliseconds
import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.feature.chat.ChatFixtures
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ChatRepositoryTest {
    private val client = FakeChatClient()
    private val store = MemoryOutboxStore()
    private var ids = 0

    private fun TestScope.repository(): LiveChatRepository {
        val dispatcher = StandardTestDispatcher(testScheduler)
        return LiveChatRepository(SESSION, client, store, this, OpenRouteGate).apply {
            clock = ChatClock(now = { 1L }, newClientId = { "c${++ids}" }, io = dispatcher)
        }
    }

    @Test
    fun anOfflineMessageWaitsInTheQueueAndLeavesWhenTheNetworkIsBack() = runTest {
        val repository = repository()
        client.sendResults += Outcome.Failure(RemoteFailure.Offline("no network"))
        client.sendResults += Outcome.Failure(RemoteFailure.Timeout)
        repository.send("  Включи свет  ", replyTo = null)
        assertEquals("Включи свет", repository.snapshot.outbox.single().text)
        advanceUntilIdle()
        assertEquals(listOf("c1", "c1", "c1"), client.sentIds)
        assertTrue(repository.snapshot.outbox.isEmpty())
        assertEquals(listOf("Включи свет", ""), repository.snapshot.messages.map { it.text })
        assertTrue(store.records.isEmpty())
    }

    @Test
    fun aRejectedMessageStaysWithItsKeyUntilRetried() = runTest {
        val repository = repository()
        client.sendResults += Outcome.Failure(RemoteFailure.Server(404, ConnectCode.UNKNOWN, "gone", reason = "chat_reply_not_found"))
        repository.send("Ответ", replyTo = null)
        advanceUntilIdle()
        assertEquals("chat_reply_not_found", repository.snapshot.outbox.single().failureKey)
        assertEquals("chat_reply_not_found", store.records.single().failureKey)
        repository.retryOutgoing("c1")
        advanceUntilIdle()
        assertTrue(repository.snapshot.outbox.isEmpty())
    }

    @Test
    fun aSavedQueueIsRestoredAndSentAfterTheFirstPage() = runTest {
        store.records = listOf(Outgoing("saved", "Из очереди", null, 5L).toRecord())
        val repository = repository()
        assertEquals(Outcome.Success(Unit), repository.refresh())
        advanceUntilIdle()
        assertEquals(listOf("saved"), client.sentIds)
        assertNull(repository.snapshot.failure)
        assertTrue(repository.snapshot.loaded)
    }

    @Test
    fun attachmentsAreUploadedOnceBeforeTheMessageIsSent() = runTest {
        val repository = repository()
        val voice = File.createTempFile("voice", ".wav").apply { writeBytes(ByteArray(64)) }
        client.uploadResults += Outcome.Failure(RemoteFailure.Offline("no network"))
        repository.send("", replyTo = null, attachments = listOf(voiceAttachment(voice)))
        advanceUntilIdle()
        assertEquals(listOf(voice.name, voice.name), client.uploads)
        assertEquals(listOf(listOf("f_${voice.name}")), client.sentAttachments)
        assertTrue(repository.snapshot.outbox.isEmpty())
    }

    @Test
    fun aVanishedFileFailsTheMessageInsteadOfLooping() = runTest {
        val repository = repository()
        val missing = File("/nonexistent/voice.wav")
        repository.send("", replyTo = null, attachments = listOf(voiceAttachment(missing)))
        advanceUntilIdle()
        assertEquals(FAILURE_FILE_MISSING, repository.snapshot.outbox.single().failureKey)
        assertTrue(client.uploads.isEmpty())
    }

    private fun voiceAttachment(file: File) =
        LocalAttachment(file.path, ChatAttachmentKind.VOICE, "audio/wav", file.name, file.length(), 1000)

    @Test
    fun aFailedFirstPageIsReported() = runTest {
        client.page = Outcome.Failure(RemoteFailure.Unauthorized)
        val repository = repository()
        repository.refresh()
        assertEquals(RemoteFailure.Unauthorized, repository.snapshot.failure)
    }

    @Test
    fun aDroppedStreamReconnectsWithoutReportingOffline() = runTest {
        val message = ChatFixtures.message("a9", 9, ChatRole.ASSISTANT, "Готово")
        client.events = flow {
            emit(ChatEvent.Upsert(message))
            throw IOException("unexpected end of stream")
        }
        val repository = LiveChatRepository(SESSION, client, store, backgroundScope, OpenRouteGate).apply {
            clock = ChatClock(
                now = { 1L },
                newClientId = { "c${++ids}" },
                backoff = Backoff(initial = 1.milliseconds, max = 10.milliseconds),
                io = StandardTestDispatcher(testScheduler),
            )
        }
        val watcher = backgroundScope.launch { repository.timeline.collect { } }

        runCurrent()
        advanceTimeBy(50)
        runCurrent()

        assertNull(repository.snapshot.failure)
        assertEquals(listOf("Готово"), repository.snapshot.messages.map { it.text })
        assertTrue(client.streams >= 2)
        watcher.cancel()
    }

    @Test
    fun theLiveStreamRunsOnlyWhileTheTimelineIsObserved() = runTest {
        val cancelled = CompletableDeferred<Unit>()
        val message = ChatFixtures.message("a1", 1, ChatRole.ASSISTANT, "Готово")
        client.events = flow {
            emit(ChatEvent.Upsert(message))
            awaitCancellation()
        }.onCompletion { cancelled.complete(Unit) }
        val repository = LiveChatRepository(SESSION, client, store, backgroundScope, OpenRouteGate)
        runCurrent()
        assertEquals(0, client.streams)

        val watcher = backgroundScope.launch { repository.timeline.collect { } }
        runCurrent()
        assertEquals(1, client.streams)
        assertEquals(listOf("Готово"), repository.timeline.value.messages.map { it.text })

        watcher.cancel()
        advanceTimeBy(LIVE_STOP_MILLIS + 1)
        assertTrue(cancelled.isCompleted)
    }

    @Test
    fun aConversationTalksToItsOwnSessionAndStopsTheReply() = runTest {
        val repository = repository()
        repository.refresh()
        repository.send("Привет", replyTo = null)
        advanceUntilIdle()
        assertEquals(listOf(SESSION), client.pagedSessions)
        assertEquals(listOf(SESSION), client.sentSessions)
        assertTrue(repository.stop("a12") is Outcome.Success)
        assertEquals(listOf("a12"), client.stopped)
    }

    private companion object {
        const val SESSION = "s_trip"
    }
}

private object OpenRouteGate : RouteGate {
    override suspend fun await() = Unit
}

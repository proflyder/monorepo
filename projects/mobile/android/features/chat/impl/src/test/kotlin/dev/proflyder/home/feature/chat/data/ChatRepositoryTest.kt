package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.NetworkWaiter
import java.io.File
import kotlinx.coroutines.ExperimentalCoroutinesApi
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

    private fun TestScope.repository(): ChatRepository {
        val dispatcher = StandardTestDispatcher(testScheduler)
        return ChatRepository(client, store, this, NetworkWaiter { true }).apply {
            clock = ChatClock(now = { 1L }, newClientId = { "c${++ids}" }, io = dispatcher)
        }
    }

    @Test
    fun anOfflineMessageWaitsInTheQueueAndLeavesWhenTheNetworkIsBack() = runTest {
        val repository = repository()
        client.sendResults += Outcome.Failure(RemoteFailure.Offline("no network"))
        client.sendResults += Outcome.Failure(RemoteFailure.Timeout)
        repository.send("  Включи свет  ", replyTo = null)
        assertEquals("Включи свет", repository.timeline.value.outbox.single().text)
        advanceUntilIdle()
        assertEquals(listOf("c1", "c1", "c1"), client.sentIds)
        assertTrue(repository.timeline.value.outbox.isEmpty())
        assertEquals(listOf("Включи свет", ""), repository.timeline.value.messages.map { it.text })
        assertTrue(store.records.isEmpty())
    }

    @Test
    fun aRejectedMessageStaysWithItsKeyUntilRetried() = runTest {
        val repository = repository()
        client.sendResults += Outcome.Failure(RemoteFailure.Server(404, "chat_reply_not_found", "gone"))
        repository.send("Ответ", replyTo = null)
        advanceUntilIdle()
        assertEquals("chat_reply_not_found", repository.timeline.value.outbox.single().failureKey)
        assertEquals("chat_reply_not_found", store.records.single().failureKey)
        repository.retryOutgoing("c1")
        advanceUntilIdle()
        assertTrue(repository.timeline.value.outbox.isEmpty())
    }

    @Test
    fun aSavedQueueIsRestoredAndSentAfterTheFirstPage() = runTest {
        store.records = listOf(Outgoing("saved", "Из очереди", null, 5L).toRecord())
        val repository = repository()
        assertEquals(Outcome.Success(Unit), repository.refresh())
        advanceUntilIdle()
        assertEquals(listOf("saved"), client.sentIds)
        assertNull(repository.timeline.value.failure)
        assertTrue(repository.timeline.value.loaded)
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
        assertTrue(repository.timeline.value.outbox.isEmpty())
    }

    @Test
    fun aVanishedFileFailsTheMessageInsteadOfLooping() = runTest {
        val repository = repository()
        val missing = File("/nonexistent/voice.wav")
        repository.send("", replyTo = null, attachments = listOf(voiceAttachment(missing)))
        advanceUntilIdle()
        assertEquals(FAILURE_FILE_MISSING, repository.timeline.value.outbox.single().failureKey)
        assertTrue(client.uploads.isEmpty())
    }

    private fun voiceAttachment(file: File) =
        LocalAttachment(file.path, ChatAttachmentKind.VOICE, "audio/wav", file.name, file.length(), 1000)

    @Test
    fun aFailedFirstPageIsReported() = runTest {
        client.page = Outcome.Failure(RemoteFailure.Unauthorized)
        val repository = repository()
        repository.refresh()
        assertEquals(RemoteFailure.Unauthorized, repository.timeline.value.failure)
    }
}

package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatEvent
import dev.proflyder.home.core.model.ChatPage
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.feature.chat.ChatFixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatTimelineTest {
    private val streaming = ChatFixtures.message("a1", 2, ChatRole.ASSISTANT, "", status = ChatStatus.PENDING)

    @Test
    fun deltasAppendOnlyWhenTheyContinueTheText() {
        val timeline = ChatTimeline().upsert(streaming)
            .apply(ChatEvent.Delta("a1", 0, "Свет "))
            .apply(ChatEvent.Delta("a1", 0, "Свет "))
            .apply(ChatEvent.Delta("a1", 5, "включён"))
            .apply(ChatEvent.Delta("a1", 40, "лишнее"))
        val message = timeline.messages.single()
        assertEquals("Свет включён", message.text)
        assertEquals(ChatStatus.STREAMING, message.status)
    }

    @Test
    fun anOlderVersionNeverOverwritesANewerOne() {
        val done = streaming.copy(text = "Готово", status = ChatStatus.DONE, version = 9)
        val timeline = ChatTimeline().upsert(done).upsert(streaming.copy(version = 3))
        assertEquals("Готово", timeline.messages.single().text)
        assertEquals(9L, timeline.cursor)
    }

    @Test
    fun aDeliveredMessageLeavesTheOutbox() {
        val queued = Outgoing("c1", "Привет", null, ChatFixtures.NOON)
        val user = ChatFixtures.message("u1", 1, ChatRole.USER, "Привет", clientId = "c1")
        val timeline = ChatTimeline().enqueue(queued).upsert(user)
        assertTrue(timeline.outbox.isEmpty())
    }

    @Test
    fun pagesMergeInSeqOrderAndKeepTheNewestCursor() {
        val newer = ChatPage(ChatFixtures.conversation.takeLast(2), hasMore = true, cursor = 20)
        val older = ChatPage(ChatFixtures.conversation.take(4), hasMore = false, cursor = 8)
        val timeline = ChatTimeline().withPage(newer).withOlder(older)
        assertEquals(listOf(1L, 2L, 3L, 4L, 5L, 6L), timeline.messages.map { it.seq })
        assertEquals(20L, timeline.cursor)
        assertEquals(false, timeline.hasMore)
    }
}

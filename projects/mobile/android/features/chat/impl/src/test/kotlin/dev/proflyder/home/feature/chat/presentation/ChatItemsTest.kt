package dev.proflyder.home.feature.chat.presentation

import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.feature.chat.ChatFixtures
import dev.proflyder.home.feature.chat.ChatFocus
import dev.proflyder.home.feature.chat.ChatLink
import dev.proflyder.home.feature.chat.ChatRoute
import dev.proflyder.home.feature.chat.data.ChatTimeline
import dev.proflyder.home.feature.chat.data.Outgoing
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatItemsTest {
    private val zone = ZoneOffset.ofHours(5)

    @Test
    fun aConversationGetsOneDayHeaderAndBubblesInOrder() {
        val items = ChatItems.build(ChatTimeline(messages = ChatFixtures.conversation), zone)
        assertTrue(items.first() is ChatItem.Day)
        val bubbles = items.filterIsInstance<ChatItem.Bubble>()
        assertEquals(6, bubbles.size)
        assertEquals(listOf(true, false, true, false, true, false), bubbles.map { it.mine })
        assertEquals(BubbleState.FAILED, bubbles.last().state)
        assertEquals("m2", bubbles[2].quote?.id)
    }

    @Test
    fun alertsBecomeCardsAndRoutesFocusAMessage() {
        val items = ChatItems.build(ChatTimeline(messages = listOf(ChatFixtures.alert("m1", 1, "critical", resolved = false))), zone)
        val card = items.filterIsInstance<ChatItem.Alert>().single()
        assertEquals("m1", card.asBubble().messageId)
        assertTrue(card.asBubble().quotable)
        assertEquals(ChatLink(ChatRoute.LEGACY_SESSION_ID, "m_7"), ChatRoute.parse("chat/message/m_7"))
        assertEquals(ChatLink("s_1", "m_7"), ChatRoute.parse("chat/session/s_1/message/m_7"))
        assertEquals(ChatLink("s_1", null), ChatRoute.parse("chat/session/s_1"))
        assertEquals(null, ChatRoute.parse("chat"))
        assertTrue(ChatRoute.isChat("chat/message/m_7"))
        ChatFocus.request("m_7")
        assertEquals("m_7", ChatFocus.take())
        assertEquals(null, ChatFocus.take())
    }

    @Test
    fun aPendingReplyIsTheTypingIndicatorNotAnEmptyBubble() {
        val pending = ChatFixtures.message("a9", 9, ChatRole.ASSISTANT, "", status = ChatStatus.PENDING)
        val items = ChatItems.build(ChatTimeline(messages = listOf(pending)), zone)
        assertEquals(ChatItem.Typing, items.last())
        assertTrue(items.none { it is ChatItem.Bubble })
    }

    @Test
    fun aQueuedMessageKeepsItsKeyWhenTheServerEchoesIt() {
        val queued = ChatItems.build(ChatTimeline(outbox = listOf(Outgoing("c7", "Привет", null, ChatFixtures.NOON))), zone)
        val echoed = ChatItems.build(
            ChatTimeline(messages = listOf(ChatFixtures.message("u1", 1, ChatRole.USER, "Привет", clientId = "c7"))),
            zone,
        )
        val before = queued.filterIsInstance<ChatItem.Bubble>().single()
        val after = echoed.filterIsInstance<ChatItem.Bubble>().single()
        assertEquals(before.key, after.key)
        assertEquals(BubbleState.QUEUED, before.state)
        assertEquals(BubbleState.SENT, after.state)
    }
}

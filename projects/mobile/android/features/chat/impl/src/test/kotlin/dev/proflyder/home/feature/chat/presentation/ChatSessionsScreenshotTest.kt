package dev.proflyder.home.feature.chat.presentation

import android.app.Activity
import android.view.LayoutInflater
import android.view.View
import androidx.recyclerview.widget.ConcatAdapter
import androidx.recyclerview.widget.LinearLayoutManager
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.glass.bindGlassBackdrop
import dev.proflyder.home.core.designsystem.list.ListHeaderSpacer
import dev.proflyder.home.core.designsystem.list.addGroupedRows
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.ui.ContentInsets
import dev.proflyder.home.core.ui.applyContentInsets
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.feature.chat.ChatFixtures
import dev.proflyder.home.feature.chat.data.ChatTimeline
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.impl.databinding.FragmentChatBinding
import dev.proflyder.home.feature.chat.impl.databinding.FragmentChatSessionsBinding
import dev.proflyder.home.feature.chat.presentation.sessions.ChatSessionsAdapter
import dev.proflyder.home.feature.chat.presentation.sessions.ChatSessionsUiState
import dev.proflyder.home.feature.chat.presentation.sessions.SessionRow
import dev.proflyder.home.feature.chat.presentation.sessions.SessionRowListener
import dev.proflyder.home.feature.chat.presentation.sessions.SessionRows
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = LIGHT)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ChatSessionsScreenshotTest {
    private object NoRows : SessionRowListener {
        override fun onOpen(row: SessionRow.Item) = Unit

        override fun onActions(row: SessionRow.Item, anchor: View) = Unit

        override fun onArchive() = Unit
    }

    private fun sessionList(name: String): FragmentChatSessionsBinding {
        val context = ChatShots.themed(AppTheme.EMERALD)
        val binding = FragmentChatSessionsBinding.inflate(LayoutInflater.from(context))
        val adapter = ChatSessionsAdapter(NoRows)
        val list = binding.chatSessionsList
        list.adapter = ConcatAdapter(ListHeaderSpacer.leading(list, binding.chatSessionsHeader), adapter)
        list.addGroupedRows(context.resources.getDimensionPixelSize(R.dimen.chat_session_divider_inset))
        val sessions = ChatFixtures.sessions
        val state = ChatSessionsUiState(
            sessions = sessions.filterNot { it.archived },
            archivedCount = sessions.count { it.archived },
            loaded = true,
        )
        adapter.submitList(SessionRows.build(state))
        binding.root.setBackgroundColor(MaterialColors.getColor(binding.root, android.R.attr.colorBackground))
        Robolectric.buildActivity(Activity::class.java).setup().get().setContentView(binding.root)
        ChatShots.applyInsets(binding.root, keyboard = 0, statusBar = 0)
        ChatShots.save(name, binding.root)
        return binding
    }

    @Test
    fun sessionListLight() {
        val binding = sessionList("chat-sessions-light")
        val active = ChatFixtures.sessions.count { !it.archived }
        assertEquals(HEADER_ARCHIVE_AND_GAP + active, binding.chatSessionsList.adapter?.itemCount)
    }

    @Test
    @Config(qualifiers = DARK)
    fun sessionListDark() {
        sessionList("chat-sessions-dark")
    }

    private val timeline = ChatTimeline(
        messages = ChatFixtures.conversation + ChatFixtures.message(
            "m7",
            7,
            ChatRole.ASSISTANT,
            "Смотрю логи `scheduler`: под перезапускался из-за OOMKilled.",
            status = ChatStatus.STREAMING,
        ),
        loaded = true,
        tools = mapOf("m7" to "kubernetes_logs"),
    )

    private fun conversation(name: String, keyboard: Boolean): FragmentChatBinding {
        val context = ChatShots.themed(AppTheme.EMERALD)
        val binding = FragmentChatBinding.inflate(LayoutInflater.from(context))
        val adapter = ChatAdapter(ChatAdapterCallbacks())
        binding.chatList.layoutManager = LinearLayoutManager(context).apply { stackFromEnd = true }
        binding.chatList.adapter = adapter
        binding.root.setBackgroundColor(MaterialColors.getColor(binding.root, android.R.attr.colorBackground))
        adapter.submitList(ChatItems.build(timeline, ZoneOffset.ofHours(5)))
        binding.chatTitle.text = ChatFixtures.sessions[2].title
        binding.chatSubtitle.text = context.getString(R.string.chat_tool_running, timeline.activeTool)
        binding.chatSubtitle.visibility = View.VISIBLE
        binding.chatStop.visibility = View.VISIBLE
        binding.chatMic.visibility = View.GONE
        Robolectric.buildActivity(Activity::class.java).setup().get().setContentView(binding.root)
        binding.chatList.applyContentInsets(ContentInsets(bottom = false))
        binding.chatPanel.pinAboveKeyboard(binding.chatList)
        binding.chatHeader.bindGlassBackdrop(binding.chatList)
        binding.chatPanel.bindGlassBackdrop(binding.chatList)
        if (keyboard) typeDraft(binding)
        val keys = if (keyboard) ChatShots.dp(binding.root, ChatShots.KEYBOARD_DP) else 0
        ChatShots.applyInsets(binding.root, keys)
        binding.chatList.scrollToPosition(adapter.itemCount - 1)
        shadowOf(android.os.Looper.getMainLooper()).idle()
        val keyColor = MaterialColors.getColor(binding.root, MaterialR.attr.colorSurfaceContainerHighest)
        ChatShots.save(name, binding.root, keys, keyColor)
        return binding
    }

    private fun typeDraft(binding: FragmentChatBinding) {
        binding.chatReplyBar.visibility = View.VISIBLE
        binding.chatExtras.visibility = View.VISIBLE
        val quote = ChatQuote("m4", ChatRole.ASSISTANT, ChatFixtures.conversation[3].text)
        binding.chatReplyAuthor.setText(chatAuthor(quote.role))
        binding.chatReplyText.text = quote.text.lines().first()
        binding.chatInput.setText(ChatFixtures.DRAFT)
        binding.chatInput.requestFocus()
        binding.chatSend.visibility = View.VISIBLE
        binding.chatStop.visibility = View.GONE
    }

    private fun assertFloating(binding: FragmentChatBinding, keyboard: Int) {
        val root = binding.root
        val status = ChatShots.dp(root, ChatShots.STATUS_BAR_DP)
        val bottom = maxOf(keyboard, ChatShots.dp(root, ChatShots.NAV_BAR_DP))
        assertTrue("header clears the status bar", binding.chatHeader.paddingTop >= status)
        assertEquals("panel sits on the keyboard", root.height - bottom, binding.chatPanel.bottom)
        assertTrue("messages scroll under the header", binding.chatList.top == 0 && binding.chatList.paddingTop >= binding.chatHeader.height)
        assertTrue("messages scroll under the input", binding.chatList.paddingBottom >= binding.chatPanel.height)
    }

    @Test
    fun conversationLight() = assertFloating(conversation("chat-conversation-light", keyboard = false), 0)

    @Test
    @Config(qualifiers = DARK)
    fun conversationDark() = assertFloating(conversation("chat-conversation-dark", keyboard = false), 0)

    @Test
    fun conversationKeyboardLight() {
        val binding = conversation("chat-conversation-keyboard-light", keyboard = true)
        assertFloating(binding, ChatShots.dp(binding.root, ChatShots.KEYBOARD_DP))
        assertTrue("the input grows to two lines", binding.chatInput.lineCount >= 2)
    }

    @Test
    @Config(qualifiers = DARK)
    fun conversationKeyboardDark() {
        val binding = conversation("chat-conversation-keyboard-dark", keyboard = true)
        assertFloating(binding, ChatShots.dp(binding.root, ChatShots.KEYBOARD_DP))
    }
}

private const val HEADER_ARCHIVE_AND_GAP = 3
private const val LIGHT = "ru-w411dp-h891dp-xxhdpi"
private const val DARK = "ru-w411dp-h891dp-night-xxhdpi"

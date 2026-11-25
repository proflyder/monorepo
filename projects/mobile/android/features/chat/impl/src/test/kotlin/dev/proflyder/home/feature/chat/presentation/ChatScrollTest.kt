package dev.proflyder.home.feature.chat.presentation

import dev.proflyder.home.core.designsystem.list.addFullWidthRows
import android.app.Activity
import android.os.Looper
import android.os.SystemClock
import android.view.LayoutInflater
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.ConcatAdapter
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.glass.bindGlassBackdrop
import dev.proflyder.home.core.designsystem.list.ListHeaderSpacer
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.ui.ContentInsets
import dev.proflyder.home.core.ui.applyContentInsets
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.core.ui.setShown
import dev.proflyder.home.core.ui.setupPullToRefresh
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-xxhdpi")
class ChatScrollTest {
    private object NoRows : SessionRowListener {
        override fun onOpen(row: SessionRow.Item) = Unit

        override fun onActions(row: SessionRow.Item, anchor: View) = Unit

        override fun onArchive() = Unit
    }

    private var refreshes = 0

    private fun sessions(count: Int): FragmentChatSessionsBinding {
        val context = ChatShots.themed(AppTheme.EMERALD)
        val binding = FragmentChatSessionsBinding.inflate(LayoutInflater.from(context))
        val adapter = ChatSessionsAdapter(NoRows)
        val list = binding.chatSessionsList
        list.adapter = ConcatAdapter(ListHeaderSpacer.leading(list, binding.chatSessionsHeader), adapter)
        list.addFullWidthRows(list.resources.getDimensionPixelSize(R.dimen.chat_session_divider_inset))
        binding.chatSessionsSearchReveal.attach(list, binding.chatSessionsSearch)
        binding.chatSessionsRefresh.setupPullToRefresh(list, binding.chatSessionsSearchReveal) { refreshes++ }
        val active = ChatFixtures.sessions.filterNot { it.archived }.take(count)
        adapter.submitList(SessionRows.build(ChatSessionsUiState(sessions = active, loaded = true)))
        Robolectric.buildActivity(Activity::class.java).setup().get().setContentView(binding.root)
        list.applyContentInsets()
        ChatShots.applyInsets(binding.root, keyboard = 0)
        idle()
        return binding
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    private fun drag(root: View, fromY: Float, toY: Float) {
        val x = root.width / 2f
        val start = SystemClock.uptimeMillis()
        root.dispatchTouchEvent(MotionEvent.obtain(start, start, MotionEvent.ACTION_DOWN, x, fromY, 0))
        idle()
        val steps = DRAG_STEPS
        repeat(steps) { step ->
            val y = fromY + (toY - fromY) * (step + 1) / steps
            root.dispatchTouchEvent(MotionEvent.obtain(start, start + step * FRAME_MS, MotionEvent.ACTION_MOVE, x, y, 0))
            idle()
        }
        root.dispatchTouchEvent(MotionEvent.obtain(start, start + steps * FRAME_MS, MotionEvent.ACTION_UP, x, toY, 0))
        idle()
    }

    @Test
    fun collapsedHeaderKeepsTheTopGap() {
        val binding = sessions(1)
        val gap = binding.root.resources.getDimensionPixelSize(R.dimen.chat_session_search_margin)
        val first = binding.chatSessionsList.getChildAt(1)
        assertEquals(gap, binding.chatSessionsHeader.height)
        assertEquals(binding.chatSessionsList.paddingTop + gap, first.top)
    }

    @Test
    fun pullOnAOneItemListRevealsTheSearch() {
        val binding = sessions(1)
        val root = binding.root
        drag(root, root.height / 3f, root.height * 2 / 3f)
        val reveal = binding.chatSessionsSearchReveal
        assertTrue(reveal.fullHeight > 0)
        assertEquals(reveal.fullHeight, reveal.offset)
        assertEquals(0, refreshes)
    }

    @Test
    fun pullOnAnEmptyListRevealsTheSearch() {
        val binding = sessions(0)
        val root = binding.root
        drag(root, root.height / 3f, root.height * 2 / 3f)
        assertEquals(binding.chatSessionsSearchReveal.fullHeight, binding.chatSessionsSearchReveal.offset)
    }

    @Test
    fun conversationScrollDoesNotRelayout() {
        val context = ChatShots.themed(AppTheme.EMERALD)
        val binding = FragmentChatBinding.inflate(LayoutInflater.from(context))
        val adapter = ChatAdapter(ChatAdapterCallbacks())
        binding.chatList.layoutManager = LinearLayoutManager(context).apply { stackFromEnd = true }
        binding.chatList.adapter = adapter
        val messages = (1..LONG_CONVERSATION).flatMap { ChatFixtures.conversation.map { m -> m.copy(id = "${m.id}-$it") } }
        adapter.submitList(ChatItems.build(ChatTimeline(messages = messages, loaded = true), ZoneOffset.UTC))
        Robolectric.buildActivity(Activity::class.java).setup().get().setContentView(binding.root)
        binding.chatList.applyContentInsets(ContentInsets(bottom = false))
        binding.chatPanel.pinAboveKeyboard(binding.chatList)
        binding.chatHeader.bindGlassBackdrop(binding.chatList)
        binding.chatPanel.bindGlassBackdrop(binding.chatList)
        binding.chatList.addOnScrollListener(ScrollDown(binding.chatScrollDown))
        ChatShots.applyInsets(binding.root, keyboard = 0)
        binding.chatList.scrollToPosition(adapter.itemCount - 1)
        idle()
        binding.chatList.scrollBy(0, -SCROLL_STEP)
        idle()
        val layouts = LayoutCounter(binding.root as ViewGroup)
        repeat(SCROLL_FRAMES) {
            binding.chatList.scrollBy(0, -SCROLL_STEP)
            idle()
        }
        assertEquals("layout passes while scrolling", 0, layouts.count)
        assertTrue(binding.chatScrollDown.isShown)
        val gap = binding.chatScrollDown.layoutParams as ViewGroup.MarginLayoutParams
        assertEquals(
            "the button rides above the composer",
            binding.chatPanel.scrimTop - gap.bottomMargin,
            binding.chatScrollDown.bottom + binding.chatScrollDown.translationY,
            1f,
        )
    }

    private class ScrollDown(private val button: View) : RecyclerView.OnScrollListener() {
        override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) = button.setShown(recyclerView.canScrollVertically(1))
    }

    private class LayoutCounter(root: ViewGroup) : View.OnLayoutChangeListener {
        var count = 0

        init {
            root.addOnLayoutChangeListener(this)
            for (index in 0 until root.childCount) root.getChildAt(index).addOnLayoutChangeListener(this)
            root.viewTreeObserver.addOnGlobalLayoutListener { count++ }
        }

        override fun onLayoutChange(v: View, l: Int, t: Int, r: Int, b: Int, ol: Int, ot: Int, or: Int, ob: Int) {
            count++
        }
    }

    private companion object {
        const val DRAG_STEPS = 20
        const val FRAME_MS = 16L
        const val LONG_CONVERSATION = 4
        const val SCROLL_STEP = 40
        const val SCROLL_FRAMES = 30
    }
}

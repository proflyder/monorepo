package dev.proflyder.home.core.designsystem

import android.app.Activity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.designsystem.list.StickyPinEdge
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class StickyHeadersLayoutManagerTest {
    private lateinit var activity: Activity
    private lateinit var list: RecyclerView
    private lateinit var manager: StickyHeadersLayoutManager
    private val rows = Rows(headers = (0 until SECTIONS).map { it * SECTION })

    @Before
    fun setUp() {
        activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        manager = StickyHeadersLayoutManager(activity)
        list = RecyclerView(activity).apply {
            layoutManager = manager
            adapter = rows
            setPadding(0, PADDING, 0, 0)
            clipToPadding = false
        }
        activity.setContentView(FrameLayout(activity).apply { addView(list, ViewGroup.LayoutParams(WIDTH, HEIGHT)) })
        settle()
    }

    @Test
    fun headerAtRestStaysAnOrdinaryItem() {
        assertNull(manager.stickyHeaderView)
        assertEquals(PADDING, manager.findViewByPosition(0)?.top)
    }

    @Test
    fun scrolledSectionHeaderPinsAtTheEdge() {
        scroll(HEADER_HEIGHT + ROW_HEIGHT)
        assertEquals(0, manager.stickyHeaderPosition)
        assertEquals(PADDING, sticky().top)
        assertEquals("0", (sticky() as TextView).text)
    }

    @Test
    fun nextHeaderPushesThePinnedOneOut() {
        val sectionHeight = HEADER_HEIGHT + (SECTION - 1) * ROW_HEIGHT
        scroll(sectionHeight - HEADER_HEIGHT / 2)
        assertEquals(0, manager.stickyHeaderPosition)
        val next = checkNotNull(list.findViewHolderForAdapterPosition(SECTION)).itemView
        assertEquals(next.top - HEADER_HEIGHT, sticky().top)
        assertEquals(PADDING - HEADER_HEIGHT / 2, sticky().top)
    }

    @Test
    fun passingTheNextHeaderPinsIt() {
        scroll(HEADER_HEIGHT + (SECTION - 1) * ROW_HEIGHT + ROW_HEIGHT)
        assertEquals(SECTION, manager.stickyHeaderPosition)
        assertEquals(PADDING, sticky().top)
    }

    @Test
    fun scrollingBackToTheTopReleasesTheHeader() {
        scroll(HEADER_HEIGHT + ROW_HEIGHT * 2)
        scroll(-(HEADER_HEIGHT + ROW_HEIGHT * 2))
        assertNull(manager.stickyHeaderView)
    }

    @Test
    fun stickyHeaderIsDrawnAboveTheRows() {
        scroll(HEADER_HEIGHT + ROW_HEIGHT)
        assertSame(sticky(), list.getChildAt(list.childCount - 1))
    }

    @Test
    fun visibilityQueriesIgnoreTheStickyCopy() {
        scroll(HEADER_HEIGHT + ROW_HEIGHT * 2)
        val natural = (0 until list.childCount).map(list::getChildAt).filter { it !== sticky() }.map(list::getChildAdapterPosition)
        assertEquals(natural.max(), manager.findLastVisibleItemPosition())
    }

    @Test
    fun adapterChangesRebindThePinnedHeader() {
        scroll(HEADER_HEIGHT + ROW_HEIGHT)
        rows.rename(0, "renamed")
        settle()
        assertEquals("renamed", (sticky() as TextView).text)
    }

    @Test
    fun removedSectionMovesThePinToTheCurrentHeader() {
        scroll(HEADER_HEIGHT + ROW_HEIGHT * 2)
        rows.insertHeader(2)
        settle()
        scroll(ROW_HEIGHT)
        assertEquals(2, manager.stickyHeaderPosition)
        assertEquals("2", (sticky() as TextView).text)
    }

    @Test
    fun scrollToPositionKeepsTheTargetBelowThePinnedHeader() {
        val target = SECTION * 2 + 2
        manager.scrollToPosition(target)
        settle()
        assertEquals(SECTION * 2, manager.stickyHeaderPosition)
        assertEquals(PADDING + HEADER_HEIGHT, manager.findViewByPosition(target)?.top)
    }

    @Test
    fun savedStateRestoresThePin() {
        scroll(HEADER_HEIGHT * 2 + (SECTION - 1) * ROW_HEIGHT + ROW_HEIGHT)
        val pinned = manager.stickyHeaderPosition
        val saved = manager.onSaveInstanceState()
        val restored = StickyHeadersLayoutManager(activity)
        restored.onRestoreInstanceState(saved)
        list.layoutManager = restored
        settle()
        assertEquals(pinned, restored.stickyHeaderPosition)
        assertEquals(PADDING, checkNotNull(restored.stickyHeaderView).top)
    }

    @Test
    fun customEdgeMovesThePin() {
        manager.pinEdge = StickyPinEdge { PADDING / 2 }
        settle()
        scroll(HEADER_HEIGHT + ROW_HEIGHT)
        assertEquals(PADDING / 2, sticky().top)
    }

    @Test
    fun replacingTheAdapterDropsThePin() {
        scroll(HEADER_HEIGHT + ROW_HEIGHT)
        list.adapter = Rows(headers = emptyList())
        settle()
        assertNull(manager.stickyHeaderView)
    }

    private fun sticky(): View = checkNotNull(manager.stickyHeaderView)

    private fun scroll(dy: Int) {
        list.scrollBy(0, dy)
        settle()
    }

    private fun settle() {
        list.measure(exactly(WIDTH), exactly(HEIGHT))
        list.layout(0, 0, WIDTH, HEIGHT)
    }

    private fun exactly(size: Int) = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private class Rows(headers: List<Int>) : RecyclerView.Adapter<RecyclerView.ViewHolder>(), SectionedAdapter {
        private val items = (0 until SECTION * SECTIONS).map { Item(it.toString(), it in headers) }.toMutableList()

        fun rename(position: Int, title: String) {
            items[position] = items[position].copy(title = title)
            notifyItemChanged(position)
        }

        fun insertHeader(position: Int) {
            items.add(position, Item(position.toString(), header = true))
            notifyItemInserted(position)
        }

        override fun isSectionHeader(position: Int): Boolean = items[position].header

        override fun getItemCount(): Int = items.size

        override fun getItemViewType(position: Int): Int = if (items[position].header) HEADER else ROW

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val height = if (viewType == HEADER) HEADER_HEIGHT else ROW_HEIGHT
            val view = TextView(parent.context).apply { layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, height) }
            return object : RecyclerView.ViewHolder(view) {}
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            (holder.itemView as TextView).text = items[position].title
        }

        private data class Item(val title: String, val header: Boolean)
    }

    private companion object {
        const val WIDTH = 400
        const val HEIGHT = 800
        const val PADDING = 60
        const val HEADER_HEIGHT = 50
        const val ROW_HEIGHT = 100
        const val SECTION = 5
        const val SECTIONS = 8
        const val HEADER = 0
        const val ROW = 1
    }
}

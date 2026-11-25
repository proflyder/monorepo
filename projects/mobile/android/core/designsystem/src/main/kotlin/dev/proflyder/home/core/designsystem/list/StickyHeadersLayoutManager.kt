package dev.proflyder.home.core.designsystem.list

import android.content.Context
import android.util.AttributeSet
import android.view.View
import androidx.recyclerview.widget.GridLayoutManager
import androidx.recyclerview.widget.RecyclerView

open class StickyHeadersLayoutManager : GridLayoutManager {
    constructor(context: Context, spanCount: Int = 1) : super(context, spanCount)

    constructor(context: Context, attrs: AttributeSet?, defStyleAttr: Int, defStyleRes: Int) :
        super(context, attrs, defStyleAttr, defStyleRes)

    private var sections: SectionedAdapter? = null
    private var list: RecyclerView? = null
    private var sticky: View? = null
    private var stickyPosition = RecyclerView.NO_POSITION
    private var stale = false
    private var detached = false
    private var observed: RecyclerView.Adapter<*>? = null
    private var pendingTarget = RecyclerView.NO_POSITION
    private val listeners = mutableListOf<StickyHeaderListener>()
    private val changes = object : RecyclerView.AdapterDataObserver() {
        override fun onChanged() = markStale()

        override fun onItemRangeChanged(positionStart: Int, itemCount: Int) = markStale()

        override fun onItemRangeInserted(positionStart: Int, itemCount: Int) = markStale()

        override fun onItemRangeRemoved(positionStart: Int, itemCount: Int) = markStale()

        override fun onItemRangeMoved(fromPosition: Int, toPosition: Int, itemCount: Int) = markStale()
    }

    var pinEdge: StickyPinEdge = StickyPinEdge.Padding
        set(value) {
            field = value
            requestLayout()
        }

    val stickyHeaderView: View? get() = sticky

    val stickyHeaderPosition: Int get() = stickyPosition

    fun addStickyHeaderListener(listener: StickyHeaderListener) {
        listeners.add(listener)
    }

    fun removeStickyHeaderListener(listener: StickyHeaderListener) {
        listeners.remove(listener)
    }

    override fun onAttachedToWindow(view: RecyclerView) {
        super.onAttachedToWindow(view)
        list = view
        observe(view.adapter)
    }

    override fun onDetachedFromWindow(view: RecyclerView, recycler: RecyclerView.Recycler) {
        super.onDetachedFromWindow(view, recycler)
        list = null
    }

    override fun onAdapterChanged(oldAdapter: RecyclerView.Adapter<*>?, newAdapter: RecyclerView.Adapter<*>?) {
        super.onAdapterChanged(oldAdapter, newAdapter)
        dropSticky(null)
        observe(newAdapter)
    }

    override fun onLayoutChildren(recycler: RecyclerView.Recycler, state: RecyclerView.State) {
        if (stale) dropSticky(recycler)
        withoutSticky { super.onLayoutChildren(recycler, state) }
        if (state.isPreLayout) return
        updateSticky(recycler)
        revealTarget(recycler, state)
    }

    override fun scrollVerticallyBy(dy: Int, recycler: RecyclerView.Recycler, state: RecyclerView.State): Int {
        val consumed = withoutSticky { super.scrollVerticallyBy(dy, recycler, state) }
        if (consumed != 0) updateSticky(recycler)
        return consumed
    }

    override fun scrollToPositionWithOffset(position: Int, offset: Int) {
        pendingTarget = position
        super.scrollToPositionWithOffset(position, offset)
    }

    override fun scrollToPosition(position: Int) = scrollToPositionWithOffset(position, 0)

    override fun findViewByPosition(position: Int): View? = withoutSticky { super.findViewByPosition(position) }

    override fun computeVerticalScrollOffset(state: RecyclerView.State): Int =
        withoutSticky { super.computeVerticalScrollOffset(state) }

    override fun computeVerticalScrollExtent(state: RecyclerView.State): Int =
        withoutSticky { super.computeVerticalScrollExtent(state) }

    override fun computeVerticalScrollRange(state: RecyclerView.State): Int =
        withoutSticky { super.computeVerticalScrollRange(state) }

    override fun findFirstVisibleItemPosition(): Int = withoutSticky { super.findFirstVisibleItemPosition() }

    override fun findFirstCompletelyVisibleItemPosition(): Int =
        withoutSticky { super.findFirstCompletelyVisibleItemPosition() }

    override fun findLastVisibleItemPosition(): Int = withoutSticky { super.findLastVisibleItemPosition() }

    override fun findLastCompletelyVisibleItemPosition(): Int =
        withoutSticky { super.findLastCompletelyVisibleItemPosition() }

    override fun onFocusSearchFailed(
        focused: View,
        direction: Int,
        recycler: RecyclerView.Recycler,
        state: RecyclerView.State,
    ): View? = withoutSticky { super.onFocusSearchFailed(focused, direction, recycler, state) }

    fun refreshSticky() {
        sticky?.let { placeSticky(it, edge()) }
    }

    private fun revealTarget(recycler: RecyclerView.Recycler, state: RecyclerView.State) {
        val target = pendingTarget
        pendingTarget = RecyclerView.NO_POSITION
        val cover = sticky ?: return
        val view = naturalChildren().firstOrNull { getPosition(it) == target } ?: return
        val hidden = getDecoratedBottom(cover) - getDecoratedTop(view)
        if (target != stickyPosition && hidden > 0) scrollVerticallyBy(-hidden, recycler, state)
    }

    private fun <T> withoutSticky(block: () -> T): T {
        val view = sticky?.takeUnless { detached } ?: return block()
        detachView(view)
        detached = true
        return runCatching(block).also {
            detached = false
            attachView(view)
        }.getOrThrow()
    }

    private fun observe(adapter: RecyclerView.Adapter<*>?) {
        if (adapter === observed) return
        observed?.unregisterAdapterDataObserver(changes)
        observed = adapter
        sections = adapter as? SectionedAdapter
        adapter?.registerAdapterDataObserver(changes)
        markStale()
    }

    private fun markStale() {
        stale = true
    }

    private fun updateSticky(recycler: RecyclerView.Recycler) {
        stale = false
        val edge = edge()
        val header = currentHeader(edge)
        if (header == null) {
            dropSticky(recycler)
            return
        }
        if (header != stickyPosition) dropSticky(recycler)
        val view = sticky ?: createSticky(recycler, header)
        placeSticky(view, edge)
    }

    private fun currentHeader(edge: Int): Int? {
        val source = sections ?: return null
        val anchor = naturalChildren().filter { getDecoratedBottom(it) > edge }.minByOrNull(::getDecoratedTop) ?: return null
        val position = getPosition(anchor)
        val header = source.stickyHeaderAtOrBefore(position) ?: return null
        val atEdge = header == position && getDecoratedTop(anchor) >= edge
        return header.takeUnless { atEdge }
    }

    private fun createSticky(recycler: RecyclerView.Recycler, position: Int): View {
        val view = recycler.getViewForPosition(position)
        addView(view)
        measureChildWithMargins(view, 0, 0)
        ignoreView(view)
        sticky = view
        stickyPosition = position
        notifyPinned()
        return view
    }

    private fun placeSticky(view: View, edge: Int) {
        val top = edge + pushOffset(view, edge)
        val left = paddingLeft + view.margins().leftMargin
        layoutDecorated(view, left, top, left + view.measuredWidth, top + view.measuredHeight)
        listeners.toList().forEach { it.onStickyHeaderMoved(view, edge) }
    }

    private fun pushOffset(view: View, edge: Int): Int {
        val source = sections ?: return 0
        val limit = edge + view.measuredHeight
        val next = naturalChildren()
            .filter { source.isStickyHeader(getPosition(it)) && getPosition(it) > stickyPosition }
            .minOfOrNull { getDecoratedTop(it) - it.margins().topMargin } ?: return 0
        return (next - limit).coerceAtMost(0)
    }

    private fun View.margins(): RecyclerView.LayoutParams = layoutParams as RecyclerView.LayoutParams

    private fun naturalChildren(): List<View> = (0 until childCount).mapNotNull(::getChildAt).filter { it !== sticky }

    private fun dropSticky(recycler: RecyclerView.Recycler?) {
        val view = sticky ?: return
        sticky = null
        stickyPosition = RecyclerView.NO_POSITION
        notifyPinned()
        stopIgnoringView(view)
        when (recycler) {
            null -> removeView(view)
            else -> removeAndRecycleView(view, recycler)
        }
    }

    private fun notifyPinned() {
        listeners.toList().forEach { it.onStickyHeaderChanged(sticky) }
    }

    private fun edge(): Int = pinEdge.edgeIn(list ?: return paddingTop)
}

fun interface StickyPinEdge {
    fun edgeIn(list: RecyclerView): Int

    companion object {
        val Padding = StickyPinEdge { it.paddingTop }
    }
}

interface StickyHeaderListener {
    fun onStickyHeaderChanged(view: View?)

    fun onStickyHeaderMoved(view: View, edge: Int)
}

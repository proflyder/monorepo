package dev.proflyder.home.core.designsystem.search

import android.view.MotionEvent
import android.view.ViewConfiguration
import androidx.recyclerview.widget.RecyclerView

internal class PullRevealGesture(
    private val search: PullRevealSearch,
    private val list: RecyclerView,
) : RecyclerView.OnScrollListener(), RecyclerView.OnItemTouchListener {
    private val slop = ViewConfiguration.get(list.context).scaledTouchSlop
    private var downY = 0f
    private var startOffset = 0
    private var active = false

    override fun onInterceptTouchEvent(rv: RecyclerView, e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> remember(e)
            MotionEvent.ACTION_MOVE -> active = shouldStart(e.y - downY)
        }
        if (active) search.beginDrag()
        return active
    }

    override fun onTouchEvent(rv: RecyclerView, e: MotionEvent) {
        when (e.actionMasked) {
            MotionEvent.ACTION_MOVE -> search.dragTo(startOffset + search.dragOffsetFor(beyondSlop(e.y - downY)))
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> finish()
        }
    }

    override fun onRequestDisallowInterceptTouchEvent(disallowIntercept: Boolean) = Unit

    override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
        if (dy > 0 && search.offset > 0) search.collapseIfIdle()
    }

    private fun remember(e: MotionEvent) {
        downY = e.y
        startOffset = search.offset
        active = false
        search.releaseFocusIfBlank()
    }

    private fun shouldStart(distance: Float): Boolean {
        val atTop = list.isAtStart()
        val pullingDown = distance > slop && search.offset < search.fullHeight
        val pushingUp = distance < -slop && search.offset > 0 && search.blank
        return active || (atTop && (pullingDown || pushingUp))
    }

    private fun beyondSlop(distance: Float): Float = when {
        distance > slop -> distance - slop
        distance < -slop -> distance + slop
        else -> 0f
    }

    private fun RecyclerView.isAtStart(): Boolean {
        val first = getChildAt(0) ?: return true
        val manager = layoutManager ?: return !canScrollVertically(-1)
        return getChildAdapterPosition(first) == 0 && manager.getDecoratedTop(first) >= paddingTop
    }

    private fun finish() {
        active = false
        search.endDrag()
    }
}

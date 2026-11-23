package dev.proflyder.home.core.designsystem.list

import android.graphics.Canvas
import android.graphics.Rect
import android.view.MotionEvent
import android.view.View
import androidx.core.graphics.withClip
import androidx.core.graphics.withTranslation
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors

class StickySectionHeaders : RecyclerView.ItemDecoration() {
    private var cached: Pair<Int, View>? = null
    private var observed: RecyclerView.Adapter<*>? = null
    private val pinned = Rect()
    private val invalidation = object : RecyclerView.AdapterDataObserver() {
        override fun onChanged() = invalidate()

        override fun onItemRangeChanged(positionStart: Int, itemCount: Int) = invalidate()

        override fun onItemRangeInserted(positionStart: Int, itemCount: Int) = invalidate()

        override fun onItemRangeRemoved(positionStart: Int, itemCount: Int) = invalidate()

        override fun onItemRangeMoved(fromPosition: Int, toPosition: Int, itemCount: Int) = invalidate()
    }

    fun attachTo(list: RecyclerView) {
        list.addItemDecoration(this)
        list.addOnItemTouchListener(PinnedTouches())
    }

    override fun onDrawOver(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        pinned.setEmpty()
        val (adapter, sections) = parent.sectionedAdapter() ?: return
        observe(adapter)
        val edge = parent.visibleTop()
        val position = parent.firstPositionBelow(edge) ?: return
        val headerPosition = sections.headerAtOrBefore(position) ?: return
        val header = headerView(parent, adapter, headerPosition)
        val y = edge + pushOffset(parent, sections, edge, edge + header.height)
        pinned.set(0, maxOf(y, edge), parent.width, y + header.height)
        canvas.withClip(pinned) { drawPinned(this, parent, header, y) }
    }

    private fun drawPinned(canvas: Canvas, parent: RecyclerView, header: View, top: Int) {
        canvas.drawColor(MaterialColors.getColor(parent, android.R.attr.colorBackground))
        canvas.withTranslation(parent.paddingLeft.toFloat(), top.toFloat()) { header.draw(this) }
    }

    private fun observe(adapter: RecyclerView.Adapter<*>) {
        if (observed === adapter) return
        observed?.unregisterAdapterDataObserver(invalidation)
        adapter.registerAdapterDataObserver(invalidation)
        observed = adapter
        invalidate()
    }

    internal fun invalidate() {
        cached = null
    }

    private fun pushOffset(parent: RecyclerView, sections: SectionedAdapter, edge: Int, limit: Int): Int {
        val next = (0 until parent.childCount).map(parent::getChildAt).firstOrNull { child ->
            child.top in (edge + 1) until limit && parent.isHeaderView(child, sections)
        }
        return next?.let { (it.top - limit).coerceAtMost(0) } ?: 0
    }

    private fun headerView(parent: RecyclerView, adapter: RecyclerView.Adapter<*>, position: Int): View {
        cached?.takeIf { it.first == position }?.let { return it.second }
        val view = createBound(parent, adapter, position)
        val width = parent.width - parent.paddingLeft - parent.paddingRight
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        view.layout(0, 0, view.measuredWidth, view.measuredHeight)
        cached = position to view
        return view
    }

    @Suppress("UNCHECKED_CAST")
    private fun createBound(parent: RecyclerView, adapter: RecyclerView.Adapter<*>, position: Int): View {
        val typed = adapter as RecyclerView.Adapter<RecyclerView.ViewHolder>
        val holder = typed.createViewHolder(parent, typed.getItemViewType(position))
        typed.bindViewHolder(holder, position)
        return holder.itemView
    }

    private inner class PinnedTouches : RecyclerView.SimpleOnItemTouchListener() {
        override fun onInterceptTouchEvent(rv: RecyclerView, e: MotionEvent): Boolean =
            e.actionMasked == MotionEvent.ACTION_DOWN && pinned.contains(e.x.toInt(), e.y.toInt())

        override fun onTouchEvent(rv: RecyclerView, e: MotionEvent) {
            if (e.actionMasked == MotionEvent.ACTION_UP && pinned.contains(e.x.toInt(), e.y.toInt())) {
                cached?.second?.performClick()
            }
        }
    }
}

internal fun RecyclerView.visibleTop(): Int = if (clipToPadding) paddingTop else 0

internal fun RecyclerView.firstPositionBelow(edge: Int): Int? = (0 until childCount)
    .map(::getChildAt)
    .filter { it.bottom > edge }
    .minByOrNull { it.top }
    ?.let(::getChildAdapterPosition)
    ?.takeIf { it != RecyclerView.NO_POSITION }

internal fun RecyclerView.isHeaderView(child: View, sections: SectionedAdapter): Boolean {
    val position = getChildAdapterPosition(child)
    return position != RecyclerView.NO_POSITION && sections.isSectionHeader(position)
}

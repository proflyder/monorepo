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
        val top = parent.getChildAt(0) ?: return
        val position = parent.getChildAdapterPosition(top).takeIf { it != RecyclerView.NO_POSITION } ?: return
        val headerPosition = sections.headerAtOrBefore(position) ?: return
        val header = headerView(parent, adapter, headerPosition)
        val y = parent.paddingTop + pushOffset(parent, sections, header.height)
        pinned.set(parent.paddingLeft, y, parent.paddingLeft + header.width, y + header.height)
        canvas.withClip(pinned) { drawPinned(this, header, MaterialColors.getColor(parent, android.R.attr.colorBackground)) }
    }

    private fun drawPinned(canvas: Canvas, header: View, background: Int) {
        canvas.drawColor(background)
        canvas.withTranslation(pinned.left.toFloat(), pinned.top.toFloat()) { header.draw(this) }
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

    private fun pushOffset(parent: RecyclerView, sections: SectionedAdapter, height: Int): Int {
        val limit = parent.paddingTop + height
        val next = (1 until parent.childCount).map(parent::getChildAt).firstOrNull { child ->
            val position = parent.getChildAdapterPosition(child)
            position != RecyclerView.NO_POSITION && sections.isSectionHeader(position) && child.top < limit
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

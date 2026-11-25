package dev.proflyder.home.core.designsystem.list

import android.content.Context
import android.graphics.Canvas
import android.graphics.Rect
import android.view.MotionEvent
import android.view.View
import androidx.core.graphics.withClip
import androidx.core.graphics.withTranslation
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.R
import dev.proflyder.home.core.designsystem.glass.GlassPainter

class StickySectionHeaders : RecyclerView.ItemDecoration() {
    private var cachedHeader: Pair<Int, View>? = null
    private var observed: RecyclerView.Adapter<*>? = null
    private val pinned = Rect()
    private var themedContext: Context? = null
    private var painter: GlassPainter? = null
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
        val glass = painterFor(parent)
        val edge = parent.visibleTop()
        val position = parent.firstPositionBelow(edge) ?: return
        val headerPosition = sections.headerAtOrBefore(position) ?: return
        val header = headerView(parent, adapter, headerPosition)
        val y = edge + pushOffset(parent, sections, edge, edge + header.height)
        val top = maxOf(y, edge)
        pinned.set(0, top, parent.width, y + header.height)
        canvas.withClip(0f, top.toFloat(), parent.width.toFloat(), top + header.height + glass.fadeHeight.toFloat()) {
            drawPinned(this, glass, parent, header, y, top)
        }
    }

    private fun drawPinned(canvas: Canvas, glass: GlassPainter, parent: RecyclerView, header: View, y: Int, top: Int) {
        val bottom = y + header.height
        glass.drawSurface(canvas, parent.width, top, bottom)
        canvas.withTranslation(parent.paddingLeft.toFloat(), y.toFloat()) { header.draw(this) }
        glass.drawEdge(canvas, parent.width, bottom)
    }

    private fun painterFor(parent: RecyclerView): GlassPainter {
        val cached = painter
        if (cached != null && themedContext === parent.context) return cached
        themedContext = parent.context
        return GlassPainter(parent.context, R.integer.ds_pinned_header_tint_alpha).also { painter = it }
    }

    private fun observe(adapter: RecyclerView.Adapter<*>) {
        if (observed === adapter) return
        observed?.unregisterAdapterDataObserver(invalidation)
        adapter.registerAdapterDataObserver(invalidation)
        observed = adapter
        invalidate()
    }

    internal fun invalidate() {
        cachedHeader = null
    }

    private fun pushOffset(parent: RecyclerView, sections: SectionedAdapter, edge: Int, limit: Int): Int {
        val next = (0 until parent.childCount).map(parent::getChildAt).firstOrNull { child ->
            child.top in (edge + 1) until limit && parent.isHeaderView(child, sections)
        }
        return next?.let { (it.top - limit).coerceAtMost(0) } ?: 0
    }

    private fun headerView(parent: RecyclerView, adapter: RecyclerView.Adapter<*>, position: Int): View {
        cachedHeader?.takeIf { it.first == position }?.let { return it.second }
        val view = createBound(parent, adapter, position)
        val width = parent.width - parent.paddingLeft - parent.paddingRight
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        view.layout(0, 0, view.measuredWidth, view.measuredHeight)
        cachedHeader = position to view
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
                cachedHeader?.second?.performClick()
            }
        }
    }
}

internal fun RecyclerView.visibleTop(): Int = paddingTop

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

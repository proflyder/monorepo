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
import dev.proflyder.home.core.designsystem.glass.AppBarHost
import dev.proflyder.home.core.designsystem.glass.GlassPainter
import dev.proflyder.home.core.designsystem.glass.PinnedBarHeader
import dev.proflyder.home.core.designsystem.glass.appBarHost

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
        list.addOnAttachStateChangeListener(ReleaseBarEdge())
    }

    override fun onDrawOver(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        val host = parent.context.appBarHost()
        val underBar = drawPinnedHeader(canvas, parent, host)
        host?.pinUnderBar(underBar)
    }

    private fun drawPinnedHeader(canvas: Canvas, parent: RecyclerView, host: AppBarHost?): PinnedBarHeader? {
        pinned.setEmpty()
        val (adapter, sections) = parent.sectionedAdapter() ?: return null
        observe(adapter)
        val edge = parent.visibleTop()
        val position = parent.firstPositionBelow(edge) ?: return null
        val headerPosition = sections.headerAtOrBefore(position) ?: return null
        val header = headerView(parent, adapter, headerPosition)
        val offset = pushOffset(parent, sections, edge, edge + header.height)
        pinned.set(0, edge, parent.width, edge + offset + header.height)
        val windowTop = parent.windowLocation()
        if (host != null && windowTop[1] + edge == host.appBarHeight) {
            return PinnedBarHeader(header, windowTop[0] + parent.paddingLeft, offset)
        }
        drawLocally(canvas, parent, header, edge, offset)
        return null
    }

    private fun drawLocally(canvas: Canvas, parent: RecyclerView, header: View, edge: Int, offset: Int) {
        val glass = painterFor(parent)
        val y = edge + offset
        canvas.withClip(0f, edge.toFloat(), parent.width.toFloat(), edge + header.height + glass.fadeHeight.toFloat()) {
            drawPinned(this, glass, parent, header, y, edge)
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

    private class ReleaseBarEdge : View.OnAttachStateChangeListener {
        override fun onViewAttachedToWindow(view: View) = Unit

        override fun onViewDetachedFromWindow(view: View) {
            view.context.appBarHost()?.pinUnderBar(null)
        }
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

private fun View.windowLocation(): IntArray = IntArray(2).also(::getLocationInWindow)

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

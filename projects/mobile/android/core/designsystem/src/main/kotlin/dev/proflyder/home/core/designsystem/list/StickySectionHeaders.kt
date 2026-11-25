package dev.proflyder.home.core.designsystem.list

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.Shader
import android.view.MotionEvent
import android.view.View
import androidx.core.graphics.ColorUtils
import androidx.core.graphics.withClip
import androidx.core.graphics.withTranslation
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.R
import com.google.android.material.R as MaterialR

class StickySectionHeaders : RecyclerView.ItemDecoration() {
    private var cachedHeader: Pair<Int, View>? = null
    private var observed: RecyclerView.Adapter<*>? = null
    private val pinned = Rect()
    private val backgroundPaint = Paint()
    private val dividerPaint = Paint()
    private val fadePaint = Paint()
    private var themedContext: Context? = null
    private var tintColor = 0
    private var dividerColor = 0
    private var fadeHeight = 0
    private var dividerHeight = 1
    private var fadeShader: Shader? = null
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
        ensureTheme(parent)
        val edge = parent.visibleTop()
        val position = parent.firstPositionBelow(edge) ?: return
        val headerPosition = sections.headerAtOrBefore(position) ?: return
        val header = headerView(parent, adapter, headerPosition)
        val y = edge + pushOffset(parent, sections, edge, edge + header.height)
        val top = maxOf(y, edge)
        pinned.set(0, top, parent.width, y + header.height)
        canvas.withClip(0f, top.toFloat(), parent.width.toFloat(), top + header.height + fadeHeight.toFloat()) {
            drawPinned(this, parent, header, y, top)
        }
    }

    private fun drawPinned(canvas: Canvas, parent: RecyclerView, header: View, y: Int, top: Int) {
        val bottom = y + header.height
        backgroundPaint.color = tintColor
        canvas.drawRect(0f, top.toFloat(), parent.width.toFloat(), bottom.toFloat(), backgroundPaint)
        canvas.withTranslation(parent.paddingLeft.toFloat(), y.toFloat()) { header.draw(this) }
        dividerPaint.color = dividerColor
        canvas.drawRect(0f, (bottom - dividerHeight).toFloat(), parent.width.toFloat(), bottom.toFloat(), dividerPaint)
        drawFade(canvas, parent, bottom)
    }

    private fun drawFade(canvas: Canvas, parent: RecyclerView, edge: Int) {
        if (fadeHeight <= 0) return
        fadePaint.shader = fadeShader
        canvas.withTranslation(0f, edge.toFloat()) {
            drawRect(0f, 0f, parent.width.toFloat(), fadeHeight.toFloat(), fadePaint)
        }
    }

    private fun ensureTheme(parent: RecyclerView) {
        if (themedContext === parent.context) return
        val resources = parent.resources
        val surface = MaterialColors.getColor(parent, MaterialR.attr.colorSurface)
        val outline = MaterialColors.getColor(parent, MaterialR.attr.colorOutline)
        tintColor = ColorUtils.setAlphaComponent(surface, resources.getInteger(R.integer.ds_pinned_header_tint_alpha))
        dividerColor = ColorUtils.setAlphaComponent(outline, resources.getInteger(R.integer.ds_pinned_header_divider_alpha))
        fadeHeight = resources.getDimensionPixelSize(R.dimen.ds_pinned_header_fade_height)
        dividerHeight = resources.getDimensionPixelSize(R.dimen.ds_glass_divider_height).coerceAtLeast(1)
        fadeShader = LinearGradient(
            0f,
            0f,
            0f,
            fadeHeight.toFloat(),
            tintColor,
            Color.TRANSPARENT,
            Shader.TileMode.CLAMP,
        )
        themedContext = parent.context
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

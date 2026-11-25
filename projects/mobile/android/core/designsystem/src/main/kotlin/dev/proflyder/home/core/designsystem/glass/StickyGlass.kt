package dev.proflyder.home.core.designsystem.glass

import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.PixelFormat
import android.graphics.drawable.Drawable
import android.view.View
import androidx.core.graphics.withTranslation
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.list.StickyHeaderListener
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager

class StickyGlass private constructor(
    private val list: RecyclerView,
    private val manager: StickyHeadersLayoutManager,
) : RecyclerView.ItemDecoration(), StickyHeaderListener, View.OnAttachStateChangeListener {
    private val style = GlassStyle(list.context)
    private var glassed: View? = null

    override fun onStickyHeaderChanged(view: View?) {
        glassed?.let(::restore)
        glassed = view?.also(::glass)
        list.glassEdgeAbove()?.coverEdge(this, view != null)
    }

    override fun onStickyHeaderMoved(view: View, edge: Int) {
        view.invalidate()
    }

    override fun onDrawOver(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        val pinned = manager.stickyHeaderView ?: return
        canvas.withTranslation(0f, pinned.translationY) { style.drawBottomEdge(this, parent.width, pinned.bottom) }
    }

    override fun onViewAttachedToWindow(view: View) = onStickyHeaderChanged(manager.stickyHeaderView)

    override fun onViewDetachedFromWindow(view: View) {
        list.glassEdgeAbove()?.coverEdge(this, false)
    }

    private fun glass(view: View) {
        view.background = PinnedGlassDrawable(GlassSurface(view.context), list, view, view.background)
    }

    private fun restore(view: View) {
        (view.background as? PinnedGlassDrawable)?.let { view.background = it.original }
    }

    companion object {
        fun attach(list: RecyclerView) {
            val manager = list.layoutManager as? StickyHeadersLayoutManager ?: return
            val glass = StickyGlass(list, manager)
            manager.addStickyHeaderListener(glass)
            list.addItemDecoration(glass)
            list.addOnAttachStateChangeListener(glass)
        }
    }
}

internal class PinnedGlassDrawable(
    private val surface: GlassSurface,
    private val list: RecyclerView,
    private val header: View,
    val original: Drawable?,
) : Drawable() {
    private val state = RecyclerView.State()

    override fun draw(canvas: Canvas) {
        surface.draw(canvas, bounds.width(), bounds.height(), ::drawRowsBehind)
        original?.let {
            it.bounds = bounds
            it.draw(canvas)
        }
    }

    private fun drawRowsBehind(canvas: Canvas) {
        canvas.withTranslation(-header.x, -header.y) {
            (0 until list.itemDecorationCount).map(list::getItemDecorationAt).filter { it !is StickyGlass }.forEach { it.onDraw(this, list, state) }
            rowsBehind().forEach { row -> withTranslation(row.x, row.y) { row.draw(this) } }
        }
    }

    private fun rowsBehind(): List<View> {
        val pinned = list.getChildAdapterPosition(header)
        return (0 until list.childCount).map(list::getChildAt).filter { it !== header && list.getChildAdapterPosition(it) != pinned }
    }

    override fun setAlpha(alpha: Int) = Unit

    override fun setColorFilter(colorFilter: ColorFilter?) = Unit

    @Deprecated("Deprecated in Drawable")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}

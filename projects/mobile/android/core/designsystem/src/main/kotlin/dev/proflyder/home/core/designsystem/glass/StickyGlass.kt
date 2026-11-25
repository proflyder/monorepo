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
import dev.proflyder.home.core.designsystem.list.StickyPinEdge

internal class StickyGlass(
    private val list: RecyclerView,
    private val manager: StickyHeadersLayoutManager,
    private val chrome: GlassChrome,
) : StickyHeaderListener, GlassChromeListener {
    private val edge = StickyPinEdge { (chrome.glassBottom - it.windowTop()).coerceAtLeast(0) }
    private var glassed: View? = null

    fun start() {
        manager.pinEdge = edge
        manager.addStickyHeaderListener(this)
        chrome.addListener(this)
        onStickyHeaderChanged(manager.stickyHeaderView)
    }

    fun stop() {
        manager.removeStickyHeaderListener(this)
        chrome.removeListener(this)
        onStickyHeaderChanged(null)
        manager.pinEdge = StickyPinEdge.Padding
    }

    override fun onGlassChromeChanged(chrome: GlassChrome) {
        manager.refreshSticky()
    }

    override fun onStickyHeaderChanged(view: View?) {
        glassed?.let(::restore)
        glassed = view?.also(::glass)
        if (view == null) chrome.pinSection(null)
    }

    override fun onStickyHeaderMoved(view: View, edge: Int) {
        view.invalidate()
        chrome.pinSection(list.windowTop() + view.bottom)
    }

    private fun glass(view: View) {
        view.background = PinnedGlassDrawable(GlassSurface(view.context), list, view, view.background)
    }

    private fun restore(view: View) {
        (view.background as? PinnedGlassDrawable)?.let { view.background = it.original }
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
            (0 until list.itemDecorationCount).forEach { list.getItemDecorationAt(it).onDraw(this, list, state) }
            rowsBehind().forEach { row ->
                withTranslation(row.x, row.y) { row.draw(this) }
            }
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

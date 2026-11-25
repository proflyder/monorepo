package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.view.ViewTreeObserver
import android.util.AttributeSet
import android.view.View
import android.view.WindowInsets
import android.widget.FrameLayout
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.children
import com.google.android.material.floatingactionbutton.FloatingActionButton
import androidx.core.view.isVisible

interface GlassHeader

class GlassHeaderLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : FrameLayout(context, attrs) {
    private var dispatchedHeight = -1
    private var received: WindowInsets? = null
    private val scrims = ImmersiveScrims(this)
    private var scrimsDrawn = false
    private var drawnFooterTop = Float.NaN
    private val footerWatcher = ViewTreeObserver.OnPreDrawListener { followFooter() }

    init {
        clipChildren = false
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnPreDrawListener(footerWatcher)
    }

    override fun onDetachedFromWindow() {
        viewTreeObserver.removeOnPreDrawListener(footerWatcher)
        super.onDetachedFromWindow()
    }

    override fun dispatchDraw(canvas: Canvas) {
        scrimsDrawn = false
        super.dispatchDraw(canvas)
    }

    override fun drawChild(canvas: Canvas, child: View, drawingTime: Long): Boolean {
        if (!scrimsDrawn && child.isImmersiveChrome()) drawScrims(canvas)
        return super.drawChild(canvas, child, drawingTime)
    }

    private fun drawScrims(canvas: Canvas) {
        scrimsDrawn = true
        immersiveHeader()?.let { scrims.drawTop(canvas, it.bottom + it.translationY) }
        val footer = footer() ?: return
        drawnFooterTop = footer.scrimTop
        scrims.drawBottom(canvas, drawnFooterTop)
    }

    private fun followFooter(): Boolean {
        val top = footer()?.scrimTop ?: Float.NaN
        if (top != drawnFooterTop) invalidate()
        anchorFabs(top)
        return true
    }

    private fun anchorFabs(footerTop: Float) {
        if (footerTop.isNaN()) return
        children.filterIsInstance<FloatingActionButton>().forEach { fab ->
            val params = fab.layoutParams as LayoutParams
            fab.translationY = footerTop - params.bottomMargin - fab.height - fab.top
        }
    }

    private fun View.isImmersiveChrome(): Boolean = this is FloatingGlassHeader || this is FloatingGlassFooter

    private fun immersiveHeader(): View? = children.firstOrNull { it is FloatingGlassHeader && it.isVisible }

    private fun footer(): FloatingGlassFooter? = children.filterIsInstance<FloatingGlassFooter>().firstOrNull { it.isVisible }

    internal val edgeOwner: GlassEdgeOwner? get() = header() as? GlassEdgeOwner

    override fun dispatchApplyWindowInsets(insets: WindowInsets): WindowInsets {
        received = insets
        val compat = WindowInsetsCompat.toWindowInsetsCompat(insets, this)
        scrims.statusInset = compat.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()).top
        scrims.navigationInset = compat.getInsets(WindowInsetsCompat.Type.navigationBars()).bottom
        val header = header()
        dispatchedHeight = header?.height ?: 0
        val below = header?.let { compat.withTop(it.height) } ?: compat
        children.forEach { child -> ViewCompat.dispatchApplyWindowInsets(child, if (child === header) compat else below) }
        return insets
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        anchorFabs(footer()?.scrimTop ?: Float.NaN)
        val height = header()?.height ?: 0
        val last = received ?: return
        if (height != dispatchedHeight) post { dispatchApplyWindowInsets(last) }
    }

    private fun header(): View? = children.filter { it is GlassHeader }.firstOrNull { it.visibility != View.GONE }

    private fun WindowInsetsCompat.withTop(top: Int): WindowInsetsCompat {
        val bars = getInsets(WindowInsetsCompat.Type.statusBars())
        val cutout = getInsets(WindowInsetsCompat.Type.displayCutout())
        return WindowInsetsCompat.Builder(this)
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(bars.left, top, bars.right, bars.bottom))
            .setInsets(WindowInsetsCompat.Type.displayCutout(), Insets.of(cutout.left, 0, cutout.right, cutout.bottom))
            .build()
    }
}

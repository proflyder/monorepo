package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.util.AttributeSet
import android.view.ViewTreeObserver
import android.widget.LinearLayout
import androidx.core.content.withStyledAttributes
import dev.proflyder.home.core.designsystem.R

class GlassChromeLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs), GlassEdgeOwner {
    private val surface = GlassSurface(context)
    private val covers = EdgeCovers(::invalidate)
    private val redraw = ViewTreeObserver.OnScrollChangedListener(::invalidate)
    private var source: GlassSource? = null
    private var edgeAtTop = false

    init {
        orientation = VERTICAL
        setWillNotDraw(false)
        receiveBorderlessRipples()
        context.withStyledAttributes(attrs, R.styleable.GlassChromeLayout) {
            edgeAtTop = getInt(R.styleable.GlassChromeLayout_glassEdge, EDGE_BOTTOM) == EDGE_TOP
        }
    }

    fun bindBackdrop(content: android.view.View) {
        source = backdropOf(content)
        invalidate()
    }

    val edgeVisible: Boolean get() = !covers.covered

    override fun coverEdge(cover: Any, covered: Boolean) = covers.coverEdge(cover, covered)

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnScrollChangedListener(redraw)
    }

    override fun onDetachedFromWindow() {
        viewTreeObserver.removeOnScrollChangedListener(redraw)
        super.onDetachedFromWindow()
    }

    override fun draw(canvas: Canvas) {
        source?.let { surface.draw(canvas, width, height, it) }
        super.draw(canvas)
        if (covers.covered) return
        if (edgeAtTop) surface.style.drawTopEdge(canvas, width, 0) else surface.style.drawBottomEdge(canvas, width, height)
    }

    private companion object {
        const val EDGE_TOP = 0
        const val EDGE_BOTTOM = 1
    }
}

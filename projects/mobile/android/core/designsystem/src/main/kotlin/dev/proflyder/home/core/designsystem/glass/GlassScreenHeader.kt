package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.util.AttributeSet
import android.view.ViewTreeObserver
import android.widget.LinearLayout
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding

class GlassScreenHeader @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs), GlassEdgeOwner, GlassHeader {
    private val surface = GlassSurface(context)
    private val covers = EdgeCovers(::invalidate)
    private val redraw = ViewTreeObserver.OnScrollChangedListener(::invalidate)
    private val baseTop = paddingTop

    init {
        orientation = VERTICAL
        setWillNotDraw(false)
        ViewCompat.setOnApplyWindowInsetsListener(this) { view, insets ->
            view.updatePadding(top = baseTop + insets.getInsets(WindowInsetsCompat.Type.systemBars()).top)
            WindowInsetsCompat.CONSUMED
        }
    }

    override fun coverEdge(cover: Any, covered: Boolean) = covers.coverEdge(cover, covered)

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnScrollChangedListener(redraw)
        context.glassEdgeHost()?.coverEdge(this, true)
    }

    override fun onDetachedFromWindow() {
        context.glassEdgeHost()?.coverEdge(this, false)
        viewTreeObserver.removeOnScrollChangedListener(redraw)
        super.onDetachedFromWindow()
    }

    override fun draw(canvas: Canvas) {
        surface.draw(canvas, width, height) { it.drawSiblingsBelow(this) }
        super.draw(canvas)
        if (!covers.covered) surface.style.drawBottomEdge(canvas, width, height)
    }
}

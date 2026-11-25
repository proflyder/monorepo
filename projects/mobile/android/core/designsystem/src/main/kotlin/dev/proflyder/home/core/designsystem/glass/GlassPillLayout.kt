package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.graphics.Outline
import android.util.AttributeSet
import android.view.View
import android.view.ViewOutlineProvider
import android.view.ViewTreeObserver
import android.widget.FrameLayout
import androidx.core.content.withStyledAttributes
import dev.proflyder.home.core.designsystem.R

class GlassPillLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : FrameLayout(context, attrs) {
    private val surface = GlassSurface(context)
    private val redraw = ViewTreeObserver.OnScrollChangedListener(::invalidate)
    private var source: GlassSource = GlassSource { it.drawSiblingsBelow(this) }
    private var cornerRadius = FULLY_ROUNDED

    init {
        setWillNotDraw(false)
        context.withStyledAttributes(attrs, R.styleable.GlassPillLayout) {
            cornerRadius = getDimension(R.styleable.GlassPillLayout_glassCornerRadius, FULLY_ROUNDED)
        }
        outlineProvider = PillOutline()
        clipToOutline = true
        receiveBorderlessRipples()
    }

    private val radius: Float get() = if (cornerRadius < 0) height / 2f else cornerRadius

    fun bindBackdrop(content: View) {
        source = backdropOf(content)
        invalidate()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnScrollChangedListener(redraw)
    }

    override fun onDetachedFromWindow() {
        viewTreeObserver.removeOnScrollChangedListener(redraw)
        super.onDetachedFromWindow()
    }

    override fun draw(canvas: Canvas) {
        surface.draw(canvas, width, height, source)
        super.draw(canvas)
        surface.style.drawOutline(canvas, width, height, radius)
    }

    private inner class PillOutline : ViewOutlineProvider() {
        override fun getOutline(view: View, outline: Outline) {
            outline.setRoundRect(0, 0, view.width, view.height, radius)
        }
    }

    private companion object {
        const val FULLY_ROUNDED = -1f
    }
}

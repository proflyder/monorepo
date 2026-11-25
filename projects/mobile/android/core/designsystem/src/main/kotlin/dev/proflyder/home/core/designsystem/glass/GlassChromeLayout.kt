package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.util.AttributeSet
import android.view.View
import android.view.ViewTreeObserver
import android.widget.LinearLayout
import androidx.core.content.withStyledAttributes
import androidx.core.graphics.withClip
import dev.proflyder.home.core.designsystem.R

class GlassChromeLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs), GlassBlockHost {
    private val surface = GlassSurface(context)
    private val redraw = ViewTreeObserver.OnScrollChangedListener(::invalidate)
    private val blocks = mutableListOf<View>()
    private val snapshots = BlockSnapshots()
    private val origin = IntArray(2)
    private val target = IntArray(2)
    private var source: GlassSource? = null
    private var edgeAtTop = false

    init {
        orientation = VERTICAL
        setWillNotDraw(false)
        context.withStyledAttributes(attrs, R.styleable.GlassChromeLayout) {
            edgeAtTop = getInt(R.styleable.GlassChromeLayout_glassEdge, EDGE_BOTTOM) == EDGE_TOP
        }
    }

    val glassBottom: Int get() = blocks.filter(View::isShown).maxOfOrNull(::bottomOf)?.coerceAtLeast(height) ?: height

    fun bindBackdrop(content: View) {
        source = ContentBackdrop(this, content)
        invalidate()
    }

    override fun attachBlock(block: View) {
        if (block in blocks) return
        blocks.add(block)
        block.alpha = 0f
        invalidate()
    }

    override fun detachBlock(block: View) {
        if (!blocks.remove(block)) return
        snapshots.release(block)
        block.alpha = 1f
        invalidate()
    }

    override fun blockMoved() = invalidate()

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnScrollChangedListener(redraw)
    }

    override fun onDetachedFromWindow() {
        viewTreeObserver.removeOnScrollChangedListener(redraw)
        super.onDetachedFromWindow()
    }

    override fun draw(canvas: Canvas) {
        val bottom = glassBottom
        source?.let { surface.draw(canvas, width, bottom, it) }
        super.draw(canvas)
        canvas.withClip(0, height, width, bottom) { blocks.filter(View::isShown).forEach { drawBlock(this, it) } }
        drawEdge(canvas, bottom)
    }

    private fun drawBlock(canvas: Canvas, block: View) {
        windowOrigin(origin)
        block.windowOrigin(target)
        snapshots.draw(canvas, block, (target[0] - origin[0]).toFloat(), (target[1] - origin[1]).toFloat())
    }

    private fun drawEdge(canvas: Canvas, bottom: Int) {
        if (edgeAtTop) surface.style.drawTopEdge(canvas, width, 0) else surface.style.drawBottomEdge(canvas, width, bottom)
    }

    private fun bottomOf(block: View): Int {
        windowOrigin(origin)
        block.windowOrigin(target)
        return target[1] - origin[1] + block.height
    }

    private companion object {
        const val EDGE_TOP = 0
        const val EDGE_BOTTOM = 1
    }
}

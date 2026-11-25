package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.os.Build
import android.util.AttributeSet
import android.view.View
import android.view.ViewTreeObserver
import androidx.annotation.RequiresApi
import dev.proflyder.home.core.designsystem.R

class GlassBarView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {
    private val blurredPainter = GlassPainter(context, R.integer.ds_glass_tint_alpha)
    private val opaquePainter = GlassPainter(context, R.integer.ds_glass_tint_alpha_fallback)
    private val blurRadius = context.resources.getDimension(R.dimen.ds_glass_blur_radius)
    private val scrollListener = ViewTreeObserver.OnScrollChangedListener { requestSnapshot() }
    private var backdrop: View? = null
    private var blur: BlurBackdrop? = null
    private var barHeight = 0
    private var statusInset = 0
    private var snapshotDirty = true
    private var edgeVisible = true

    init {
        setWillNotDraw(false)
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
    }

    fun bindBackdrop(view: View) {
        backdrop = view
        requestSnapshot()
    }

    fun requestSnapshot() {
        snapshotDirty = true
        postInvalidateOnAnimation()
    }

    fun setEdgeVisible(visible: Boolean) {
        if (visible == edgeVisible) return
        edgeVisible = visible
        invalidate()
    }

    fun setBarHeight(height: Int, topInset: Int) {
        if (height == barHeight && topInset == statusInset) return
        barHeight = height
        statusInset = topInset
        updateOwnHeight()
        requestSnapshot()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnScrollChangedListener(scrollListener)
        snapshotDirty = true
    }

    override fun onDetachedFromWindow() {
        viewTreeObserver.removeOnScrollChangedListener(scrollListener)
        blur = null
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        if (barHeight <= 0 || width <= 0) return
        val content = backdrop
        val painter = when {
            content != null && canBlur(canvas) -> blurredPainter.also { drawBlurred(canvas, content) }
            else -> opaquePainter
        }
        opaquePainter.drawSurface(canvas, width, 0, statusInset)
        painter.drawSurface(canvas, width, statusInset, barHeight)
        if (edgeVisible) painter.drawEdge(canvas, width, barHeight)
        snapshotDirty = false
    }

    private fun canBlur(canvas: Canvas) = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && canvas.isHardwareAccelerated

    @RequiresApi(Build.VERSION_CODES.S)
    private fun drawBlurred(canvas: Canvas, content: View) {
        val snapshot = blur ?: BlurBackdrop(blurRadius).also { blur = it }
        snapshot.draw(canvas, content, width, barHeight, snapshotDirty)
    }

    private fun updateOwnHeight() {
        val wanted = barHeight + opaquePainter.fadeHeight
        val params = layoutParams ?: return
        if (params.height == wanted) return
        params.height = wanted
        layoutParams = params
    }
}

package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.os.Build
import android.util.AttributeSet
import android.view.View
import android.view.ViewTreeObserver
import androidx.annotation.RequiresApi
import androidx.core.graphics.withClip
import androidx.core.graphics.withTranslation
import dev.proflyder.home.core.designsystem.R

class GlassBarView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {
    internal val blurredPainter = GlassPainter(context, R.integer.ds_glass_tint_alpha)
    private val opaquePainter = GlassPainter(context, R.integer.ds_glass_tint_alpha_fallback)
    internal val blurRadius = context.resources.getDimension(R.dimen.ds_glass_blur_radius)
    private val scrollListener = ViewTreeObserver.OnScrollChangedListener { requestSnapshot() }
    private var backdrop: View? = null
    private var blur: BlurBackdrop? = null
    private var barHeight = 0
    private var statusInset = 0
    private var pinned: PinnedBarHeader? = null
    private var recording = false
    private var blurring = true
    internal var snapshotDirty = true
        private set

    internal val surfaceBottom: Int get() = barHeight + (pinned?.visibleHeight ?: 0)

    init {
        setWillNotDraw(false)
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
    }

    fun bindBackdrop(view: View) {
        backdrop = view
        requestSnapshot()
    }

    fun requestSnapshot() {
        if (recording) return
        snapshotDirty = true
        if (blurring) postInvalidateOnAnimation()
    }

    fun setPinnedHeader(header: PinnedBarHeader?) {
        if (header == pinned) return
        pinned = header
        refreshSurface()
    }

    fun setBarHeight(height: Int, topInset: Int) {
        if (height == barHeight && topInset == statusInset) return
        barHeight = height
        statusInset = topInset
        refreshSurface()
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
        val bottom = surfaceBottom
        val content = backdrop?.takeIf { canvas.isHardwareAccelerated }
        blurring = content != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
        val painter = if (blurring) blurredPainter else opaquePainter
        if (content != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) drawBlurred(canvas, content, bottom)
        opaquePainter.drawSurface(canvas, width, 0, statusInset)
        painter.drawSurface(canvas, width, statusInset, bottom)
        pinned?.let { drawPinned(canvas, it) }
        painter.drawEdge(canvas, width, bottom)
        snapshotDirty = false
    }

    private fun drawPinned(canvas: Canvas, header: PinnedBarHeader) {
        canvas.withClip(0, barHeight, width, surfaceBottom) {
            withTranslation(header.left.toFloat(), (barHeight + header.offset).toFloat()) { header.view.draw(this) }
        }
    }

    @RequiresApi(Build.VERSION_CODES.S)
    private fun drawBlurred(canvas: Canvas, content: View, bottom: Int) {
        val snapshot = blur ?: BlurBackdrop(blurRadius).also { blur = it }
        recording = true
        runCatching { snapshot.draw(canvas, content, width, bottom, snapshotDirty) }
            .also { recording = false }
            .getOrThrow()
    }

    private fun refreshSurface() {
        snapshotDirty = true
        invalidate()
    }
}

package dev.proflyder.home.core.designsystem

import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.drawable.Drawable
import com.google.android.material.button.MaterialButton
import com.google.android.material.progressindicator.CircularProgressIndicatorSpec
import com.google.android.material.progressindicator.IndeterminateDrawable

private class LoadingMemory(
    val text: CharSequence?,
    val icon: Drawable?,
    val iconGravity: Int,
    val iconPadding: Int,
    val minWidth: Int,
    val clickable: Boolean,
    val paddingStart: Int,
    val paddingEnd: Int,
)

val MaterialButton.isLoading: Boolean
    get() = getTag(R.id.ds_button_loading_text) is LoadingMemory

fun MaterialButton.setLoading(loading: Boolean) {
    when {
        loading == isLoading -> Unit
        loading -> startLoading()
        else -> stopLoading()
    }
}

private fun MaterialButton.startLoading() {
    setTag(R.id.ds_button_loading_text, LoadingMemory(text, icon, iconGravity, iconPadding, minWidth, isClickable, paddingStart, paddingEnd))
    minWidth = width
    icon = loadingDrawable()
    iconGravity = MaterialButton.ICON_GRAVITY_START
    setPaddingRelative(((width - iconSize) / 2).coerceAtLeast(0), paddingTop, 0, paddingBottom)
    iconPadding = 0
    text = null
    isClickable = false
}

private fun MaterialButton.stopLoading() {
    val memory = getTag(R.id.ds_button_loading_text) as LoadingMemory
    setTag(R.id.ds_button_loading_text, null)
    icon = memory.icon
    iconGravity = memory.iconGravity
    iconPadding = memory.iconPadding
    text = memory.text
    minWidth = memory.minWidth
    setPaddingRelative(memory.paddingStart, paddingTop, memory.paddingEnd, paddingBottom)
    isClickable = memory.clickable
}

private fun MaterialButton.loadingDrawable(): IndeterminateDrawable<CircularProgressIndicatorSpec> {
    val spec = CircularProgressIndicatorSpec(context, null, 0, LOADING_STYLE)
    spec.indicatorColors = intArrayOf(currentTextColor)
    return IndeterminateDrawable.createCircularDrawable(context, spec)
}

internal fun MaterialButton.freezeLoadingFrame() {
    icon = FrozenArc(currentTextColor, resources.displayMetrics.density * ARC_STROKE_DP)
}

private class FrozenArc(color: Int, stroke: Float) : Drawable() {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        this.color = color
        style = Paint.Style.STROKE
        strokeWidth = stroke
        strokeCap = Paint.Cap.ROUND
    }
    private val oval = RectF()

    override fun draw(canvas: Canvas) {
        oval.set(bounds)
        oval.inset(paint.strokeWidth, paint.strokeWidth)
        canvas.drawArc(oval, ARC_START, ARC_SWEEP, false, paint)
    }

    override fun setAlpha(alpha: Int) {
        paint.alpha = alpha
    }

    override fun setColorFilter(colorFilter: ColorFilter?) {
        paint.colorFilter = colorFilter
    }

    @Deprecated("Deprecated in Java")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}

private const val ARC_STROKE_DP = 2f
private const val ARC_START = -90f
private const val ARC_SWEEP = 270f

private val LOADING_STYLE = com.google.android.material.R.style.Widget_Material3_CircularProgressIndicator_ExtraSmall

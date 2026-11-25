package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.os.Build
import android.util.AttributeSet
import android.view.View
import android.view.ViewTreeObserver
import androidx.annotation.RequiresApi
import androidx.core.graphics.ColorUtils
import androidx.core.graphics.withTranslation
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.R

class GlassBarView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {
    private val tintPaint = Paint()
    private val dividerPaint = Paint()
    private val fadePaint = Paint()
    private val surfaceColor = MaterialColors.getColor(context, com.google.android.material.R.attr.colorSurface, 0)
    private val outlineColor = MaterialColors.getColor(context, com.google.android.material.R.attr.colorOutline, 0)
    private val tintColor = ColorUtils.setAlphaComponent(
        surfaceColor,
        context.resources.getInteger(R.integer.ds_glass_tint_alpha),
    )
    private val fallbackTintColor = ColorUtils.setAlphaComponent(
        surfaceColor,
        context.resources.getInteger(R.integer.ds_glass_tint_alpha_fallback),
    )
    private val fadeHeight = context.resources.getDimensionPixelSize(R.dimen.ds_glass_fade_height)
    private val dividerHeight = context.resources.getDimensionPixelSize(R.dimen.ds_glass_divider_height).coerceAtLeast(1)
    private val blurRadius = context.resources.getDimension(R.dimen.ds_glass_blur_radius)
    private val scrollListener = ViewTreeObserver.OnScrollChangedListener { invalidateSnapshot() }
    private var backdrop: View? = null
    private var blur: BlurBackdrop? = null
    private var barHeight = -1
    private var snapshotDirty = true
    private var fadeShader: Shader? = null
    private var fadeShaderColor = 0

    init {
        setWillNotDraw(false)
        isClickable = false
        isFocusable = false
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
        dividerPaint.color = outlineColor
        fadePaint.isAntiAlias = false
    }

    fun bindBackdrop(view: View) {
        backdrop = view
        snapshotDirty = true
        invalidate()
    }

    fun requestSnapshot() {
        snapshotDirty = true
        postInvalidateOnAnimation()
    }

    fun setBarHeight(height: Int) {
        if (height == barHeight) return
        barHeight = height
        snapshotDirty = true
        updateOwnHeight()
        invalidate()
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

    override fun onSizeChanged(width: Int, height: Int, oldWidth: Int, oldHeight: Int) {
        super.onSizeChanged(width, height, oldWidth, oldHeight)
        snapshotDirty = true
        fadeShader = null
        fadeShaderColor = 0
    }

    override fun onDraw(canvas: Canvas) {
        val width = width
        if (barHeight <= 0 || width <= 0) return
        val content = backdrop
        val barTint = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            if (content != null && canvas.isHardwareAccelerated) {
                drawBlurred(canvas, content, width)
                tintColor
            } else {
                drawTint(canvas, width, fallbackTintColor)
                fallbackTintColor
            }
        } else {
            drawTint(canvas, width, fallbackTintColor)
            fallbackTintColor
        }
        drawDivider(canvas, width)
        drawFade(canvas, width, barTint)
        snapshotDirty = false
    }

    @RequiresApi(Build.VERSION_CODES.S)
    private fun drawBlurred(canvas: Canvas, content: View, width: Int) {
        val snapshot = blur ?: BlurBackdrop(blurRadius).also { blur = it }
        snapshot.draw(canvas, content, width, barHeight, snapshotDirty)
        drawTint(canvas, width, tintColor)
    }

    private fun drawTint(canvas: Canvas, width: Int, color: Int) {
        tintPaint.color = color
        canvas.drawRect(0f, 0f, width.toFloat(), barHeight.toFloat(), tintPaint)
    }

    private fun drawDivider(canvas: Canvas, width: Int) {
        val bottom = barHeight.toFloat()
        canvas.drawRect(0f, bottom - dividerHeight, width.toFloat(), bottom, dividerPaint)
    }

    private fun drawFade(canvas: Canvas, width: Int, color: Int) {
        if (fadeHeight <= 0) return
        if (fadeShader == null || fadeShaderColor != color) {
            fadeShader = LinearGradient(
                0f,
                0f,
                0f,
                fadeHeight.toFloat(),
                color,
                Color.TRANSPARENT,
                Shader.TileMode.CLAMP,
            )
            fadeShaderColor = color
        }
        fadePaint.shader = fadeShader
        canvas.withTranslation(0f, barHeight.toFloat()) {
            drawRect(0f, 0f, width.toFloat(), fadeHeight.toFloat(), fadePaint)
        }
    }

    private fun updateOwnHeight() {
        val wanted = barHeight + fadeHeight
        val params = layoutParams ?: return
        if (params.height == wanted) return
        params.height = wanted
        layoutParams = params
    }

    private fun invalidateSnapshot() {
        snapshotDirty = true
        postInvalidateOnAnimation()
    }
}

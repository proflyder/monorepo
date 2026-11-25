package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import androidx.annotation.IntegerRes
import androidx.core.graphics.ColorUtils
import androidx.core.graphics.withTranslation
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.R
import com.google.android.material.R as MaterialR

class GlassPainter(context: Context, @IntegerRes tintAlpha: Int) {
    private val surface = MaterialColors.getColor(context, MaterialR.attr.colorSurface, Color.BLACK)
    internal val tintAlpha = context.resources.getInteger(tintAlpha)
    private val tintPaint = Paint().apply { color = ColorUtils.setAlphaComponent(surface, this@GlassPainter.tintAlpha) }
    private val dividerPaint = Paint().apply {
        color = MaterialColors.getColor(context, MaterialR.attr.colorOutlineVariant, Color.GRAY)
    }
    private val fadePaint = Paint()
    private val dividerHeight = context.resources.getDimensionPixelSize(R.dimen.ds_glass_divider_height).coerceAtLeast(1)
    val fadeHeight = context.resources.getDimensionPixelSize(R.dimen.ds_glass_fade_height)

    init {
        fadePaint.shader = LinearGradient(
            0f,
            0f,
            0f,
            fadeHeight.toFloat(),
            ColorUtils.setAlphaComponent(surface, context.resources.getInteger(R.integer.ds_glass_fade_alpha)),
            Color.TRANSPARENT,
            Shader.TileMode.CLAMP,
        )
    }

    fun drawSurface(canvas: Canvas, width: Int, top: Int, bottom: Int) {
        canvas.drawRect(0f, top.toFloat(), width.toFloat(), bottom.toFloat(), tintPaint)
    }

    fun drawEdge(canvas: Canvas, width: Int, bottom: Int) {
        canvas.drawRect(0f, (bottom - dividerHeight).toFloat(), width.toFloat(), bottom.toFloat(), dividerPaint)
        if (fadeHeight <= 0) return
        canvas.withTranslation(0f, bottom.toFloat()) {
            drawRect(0f, 0f, width.toFloat(), fadeHeight.toFloat(), fadePaint)
        }
    }
}

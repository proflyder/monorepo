package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import androidx.core.graphics.ColorUtils
import androidx.core.graphics.withScale
import androidx.core.graphics.withTranslation
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.R
import com.google.android.material.R as MaterialR

class GlassStyle(context: Context) {
    private val resources = context.resources
    private val surface = MaterialColors.getColor(context, MaterialR.attr.colorSurface, Color.BLACK)
    val backdropColor = MaterialColors.getColor(context, android.R.attr.colorBackground, surface)
    val blurRadius = resources.getDimension(R.dimen.ds_glass_blur_radius)
    val blurredTintAlpha = resources.getInteger(R.integer.ds_glass_tint_alpha)
    val fallbackTintAlpha = resources.getInteger(R.integer.ds_glass_tint_alpha_fallback)
    private val blurredTint = Paint().apply { color = ColorUtils.setAlphaComponent(surface, blurredTintAlpha) }
    private val fallbackTint = Paint().apply { color = ColorUtils.setAlphaComponent(surface, fallbackTintAlpha) }
    private val hairline = Paint().apply {
        color = MaterialColors.getColor(context, MaterialR.attr.colorOutlineVariant, Color.GRAY)
    }
    private val hairlineHeight = resources.getDimension(R.dimen.ds_glass_divider_height)
    val fadeHeight = resources.getDimensionPixelSize(R.dimen.ds_glass_fade_height)
    private val fade = Paint().apply {
        shader = LinearGradient(
            0f,
            0f,
            0f,
            fadeHeight.toFloat(),
            ColorUtils.setAlphaComponent(backdropColor, resources.getInteger(R.integer.ds_glass_fade_alpha)),
            Color.TRANSPARENT,
            Shader.TileMode.CLAMP,
        )
    }

    fun drawTint(canvas: Canvas, width: Int, top: Int, bottom: Int, blurred: Boolean) {
        val paint = if (blurred) blurredTint else fallbackTint
        canvas.drawRect(0f, top.toFloat(), width.toFloat(), bottom.toFloat(), paint)
    }

    fun drawBottomEdge(canvas: Canvas, width: Int, bottom: Int) {
        canvas.withTranslation(0f, bottom.toFloat()) {
            drawRect(0f, 0f, width.toFloat(), fadeHeight.toFloat(), fade)
            drawRect(0f, -hairlineHeight, width.toFloat(), 0f, hairline)
        }
    }

    fun drawTopEdge(canvas: Canvas, width: Int, top: Int) {
        canvas.withTranslation(0f, top.toFloat()) {
            withScale(1f, -1f) { drawRect(0f, 0f, width.toFloat(), fadeHeight.toFloat(), fade) }
            drawRect(0f, 0f, width.toFloat(), hairlineHeight, hairline)
        }
    }
}

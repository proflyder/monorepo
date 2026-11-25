package dev.proflyder.home.core.designsystem.glass

import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.view.View
import androidx.core.graphics.ColorUtils
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.R

internal class ImmersiveScrims(private val host: View) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val fade = host.resources.getDimension(R.dimen.ds_scrim_fade_height)
    private val color get() = MaterialColors.getColor(host, android.R.attr.colorBackground)
    var statusInset = 0
    var navigationInset = 0

    fun drawTop(canvas: Canvas, headerBottom: Float) {
        val end = headerBottom + fade
        if (end <= 0f) return
        val opaqueUntil = (statusInset / end).coerceIn(0f, 1f)
        val base = color
        val colors = intArrayOf(base, base, transparent(base))
        paint.shader = LinearGradient(0f, 0f, 0f, end, colors, floatArrayOf(0f, opaqueUntil, 1f), Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, host.width.toFloat(), end, paint)
    }

    fun drawBottom(canvas: Canvas, footerTop: Float) {
        val height = host.height.toFloat()
        val start = (footerTop - fade).coerceAtMost(height)
        val span = height - start
        if (span <= 0f) return
        val opaqueFrom = ((span - navigationInset) / span).coerceIn(0f, 1f)
        val base = color
        val colors = intArrayOf(transparent(base), base, base)
        paint.shader = LinearGradient(0f, start, 0f, height, colors, floatArrayOf(0f, opaqueFrom, 1f), Shader.TileMode.CLAMP)
        canvas.drawRect(0f, start, host.width.toFloat(), height, paint)
    }

    private fun transparent(base: Int): Int = ColorUtils.setAlphaComponent(base, Color.TRANSPARENT)
}

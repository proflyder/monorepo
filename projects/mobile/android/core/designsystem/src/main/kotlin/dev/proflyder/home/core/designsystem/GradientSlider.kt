package dev.proflyder.home.core.designsystem

import android.content.Context
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.util.AttributeSet
import android.view.View
import com.google.android.material.slider.Slider

class GradientSlider @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = com.google.android.material.R.attr.sliderStyle,
) : Slider(context, attrs, defStyleAttr) {
    private val trackPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private var trackColors: IntArray = IntArray(0)
    private var shaderWidth = -1
    private var shaderRtl = false

    fun setTrackGradient(colors: IntArray) {
        trackColors = colors
        shaderWidth = -1
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        drawGradientTrack(canvas)
        super.onDraw(canvas)
    }

    private fun drawGradientTrack(canvas: Canvas) {
        if (trackColors.size < MIN_STOPS || trackWidth <= 0) return
        updateShader()
        val start = trackSidePadding.toFloat()
        val centerY = height / 2f
        val halfHeight = trackHeight / 2f
        canvas.drawRoundRect(
            start,
            centerY - halfHeight,
            start + trackWidth,
            centerY + halfHeight,
            halfHeight,
            halfHeight,
            trackPaint,
        )
    }

    private fun updateShader() {
        val rtl = layoutDirection == View.LAYOUT_DIRECTION_RTL
        if (shaderWidth == trackWidth && shaderRtl == rtl) return
        val start = trackSidePadding.toFloat()
        val end = start + trackWidth
        trackPaint.shader = LinearGradient(
            if (rtl) end else start,
            0f,
            if (rtl) start else end,
            0f,
            trackColors,
            null,
            Shader.TileMode.CLAMP,
        )
        shaderWidth = trackWidth
        shaderRtl = rtl
    }

    private companion object {
        const val MIN_STOPS = 2
    }
}

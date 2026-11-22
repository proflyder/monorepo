package dev.proflyder.home.feature.chat.presentation

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.AttributeSet
import android.view.View
import com.google.android.material.color.MaterialColors

class VoiceWaveView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {
    private val played = Paint(Paint.ANTI_ALIAS_FLAG)
    private val rest = Paint(Paint.ANTI_ALIAS_FLAG)
    private val bar = RectF()
    private var levels: FloatArray = FloatArray(BARS) { FLAT }
    private var progress = 0f

    init {
        played.color = MaterialColors.getColor(this, androidx.appcompat.R.attr.colorPrimary)
        rest.color = MaterialColors.getColor(this, com.google.android.material.R.attr.colorOutline)
    }

    internal fun setLevels(values: FloatArray?) {
        levels = values?.takeIf { it.isNotEmpty() } ?: FloatArray(BARS) { FLAT }
        invalidate()
    }

    internal fun setProgress(value: Float) {
        progress = value.coerceIn(0f, 1f)
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        val step = width.toFloat() / levels.size
        val barWidth = step * BAR_FILL
        val playedBars = progress * levels.size
        levels.forEachIndexed { index, level -> drawBar(canvas, index, step, barWidth, level, index < playedBars) }
    }

    private fun drawBar(canvas: Canvas, index: Int, step: Float, barWidth: Float, level: Float, done: Boolean) {
        val heightPx = maxOf(barWidth, height * level)
        val left = index * step
        bar.set(left, (height - heightPx) / 2, left + barWidth, (height + heightPx) / 2)
        canvas.drawRoundRect(bar, barWidth / 2, barWidth / 2, if (done) played else rest)
    }

    companion object {
        const val BARS = 36
        private const val FLAT = 0.15f
        private const val BAR_FILL = 0.6f
    }
}

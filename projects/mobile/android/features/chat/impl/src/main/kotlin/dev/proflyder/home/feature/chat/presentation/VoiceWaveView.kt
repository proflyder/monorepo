package dev.proflyder.home.feature.chat.presentation

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.AttributeSet
import android.view.View
import com.google.android.material.color.MaterialColors
import java.io.File
import java.io.RandomAccessFile
import kotlin.math.abs

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

internal object VoiceLevels {
    private const val HEADER_BYTES = 44L
    private const val FULL_SCALE = 32768f
    private const val FLOOR = 0.12f

    fun of(file: File, bars: Int = VoiceWaveView.BARS): FloatArray? {
        val pcm = RandomAccessFile(file, "r").use { read(it) } ?: return null
        val samples = pcm.size / 2
        if (samples < bars) return null
        val peaks = FloatArray(bars) { bucket -> peak(pcm, samples * bucket / bars, samples * (bucket + 1) / bars) }
        val loudest = peaks.maxOrNull()?.takeIf { it > 0f } ?: return null
        return FloatArray(bars) { maxOf(FLOOR, peaks[it] / loudest) }
    }

    private fun read(file: RandomAccessFile): ByteArray? {
        if (file.length() <= HEADER_BYTES) return null
        file.seek(HEADER_BYTES)
        return ByteArray((file.length() - HEADER_BYTES).toInt()).also { file.readFully(it) }
    }

    private fun peak(pcm: ByteArray, from: Int, to: Int): Float =
        (from until to).maxOfOrNull { abs(sample(pcm, it)) }?.div(FULL_SCALE) ?: 0f

    private fun sample(pcm: ByteArray, index: Int): Int =
        (pcm[index * 2].toInt() and 0xFF) or (pcm[index * 2 + 1].toInt() shl 8)
}

package dev.proflyder.home.feature.chat.data

import java.io.File
import java.io.RandomAccessFile
import kotlin.math.abs

internal object VoiceLevels {
    private const val HEADER_BYTES = 44L
    private const val FULL_SCALE = 32768f
    private const val FLOOR = 0.12f

    fun of(file: File, bars: Int): FloatArray? {
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

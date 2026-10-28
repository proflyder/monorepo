package dev.proflyder.home.core.time

import java.util.Locale
import kotlin.math.max

/**
 * Parses user-entered timer durations and formats countdowns.
 *
 * Accepted forms:
 *  - unit forms: `30s`, `5m`, `2h`, `1h 30m`, `1h30m`, `2m 15s`
 *  - clock forms: `mm:ss`, `hh:mm:ss`
 *
 * A duration must be from 1 second through 7 days (the scheduler contract).
 */
object DurationText {
    const val MIN_SECONDS = 1L
    const val MAX_SECONDS = 7L * 24L * 60L * 60L


    sealed interface Result {
        data class Valid(val seconds: Long) : Result
        data object Empty : Result
        data class OutOfRange(val seconds: Long) : Result
        data object Malformed : Result
    }

    fun parse(raw: String): Result {
        val text = raw.trim().lowercase(Locale.ROOT)
        if (text.isEmpty()) return Result.Empty
        if (text.any { it.isISOControl() }) return Result.Malformed

        val parsed = if (text.contains(':')) parseClock(text) else parseUnits(text)
        val seconds = parsed ?: return Result.Malformed

        return when {
            seconds < MIN_SECONDS -> Result.OutOfRange(seconds)
            seconds > MAX_SECONDS -> Result.OutOfRange(seconds)
            else -> Result.Valid(seconds)
        }
    }

    fun parseOrNull(raw: String): Long? = (parse(raw) as? Result.Valid)?.seconds

    private fun parseClock(text: String): Long? {
        val parts = text.split(':')
        if (parts.size !in 2..3) return null
        if (parts.any { it.isEmpty() || it.length > 2 || !it.all(Char::isDigit) }) return null
        val numbers = parts.map { it.toLong() }
        val (hours, minutes, secs) = when (numbers.size) {
            2 -> Triple(0L, numbers[0], numbers[1])
            else -> Triple(numbers[0], numbers[1], numbers[2])
        }
        if (minutes !in 0..59 || secs !in 0..59) return null
        return hours * 3600L + minutes * 60L + secs
    }

    private fun parseUnits(text: String): Long? {
        var total = 0L
        var index = 0
        var sawPart = false
        while (index < text.length) {
            while (index < text.length && text[index] == ' ') index++
            if (index >= text.length) break
            val numberStart = index
            while (index < text.length && text[index].isDigit()) index++
            if (index == numberStart) return null
            val value = text.substring(numberStart, index).toLongOrNull() ?: return null
            while (index < text.length && text[index] == ' ') index++
            if (index >= text.length) return null
            val multiplier = when (text[index]) {
                'h' -> 3600L
                'm' -> 60L
                's' -> 1L
                else -> return null
            }
            index++
            total += value * multiplier
            sawPart = true
        }
        return if (sawPart) total else null
    }

    /** `mm:ss` below one hour, `h:mm:ss` otherwise. Never negative. */
    fun format(totalSeconds: Long): String {
        val safe = max(0L, totalSeconds)
        val hours = safe / 3600L
        val minutes = (safe % 3600L) / 60L
        val seconds = safe % 60L
        return if (hours > 0) {
            String.format(Locale.ROOT, "%d:%02d:%02d", hours, minutes, seconds)
        } else {
            String.format(Locale.ROOT, "%02d:%02d", minutes, seconds)
        }
    }

    /** Compact human label for presets, e.g. `1 h 30 min`. */
    fun describe(totalSeconds: Long): String {
        val safe = max(0L, totalSeconds)
        if (safe == 0L) return "0 s"
        val hours = safe / 3600L
        val minutes = (safe % 3600L) / 60L
        val seconds = safe % 60L
        val parts = buildList {
            if (hours > 0) add("$hours h")
            if (minutes > 0) add("$minutes min")
            if (seconds > 0) add("$seconds s")
        }
        return parts.joinToString(" ")
    }
}

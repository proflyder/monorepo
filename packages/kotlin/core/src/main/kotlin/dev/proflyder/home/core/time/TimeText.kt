package dev.proflyder.home.core.time

import java.time.DateTimeException
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/** Strict parsing for wall-clock time, date, and IANA timezone input. */
object TimeText {
    private val dateFormatter: DateTimeFormatter = DateTimeFormatter.ISO_LOCAL_DATE

    data class TimeResult(val hour: Int, val minute: Int) {
        val asLocalTime: LocalTime get() = LocalTime.of(hour, minute)
        val display: String get() = formatTime(hour, minute)
    }

    sealed interface ParseResult<out T> {
        data class Valid<T>(val value: T) : ParseResult<T>
        data class Invalid(val reason: TimeProblem) : ParseResult<Nothing>
    }

    enum class TimeProblem {
        EMPTY,
        MALFORMED,
        HOUR_OUT_OF_RANGE,
        MINUTE_OUT_OF_RANGE,
        DATE_MALFORMED,
        DATE_IN_PAST,
        UNKNOWN_TIMEZONE,
    }

    fun parseTime(raw: String): ParseResult<TimeResult> {
        val text = raw.trim()
        if (text.isEmpty()) return ParseResult.Invalid(TimeProblem.EMPTY)
        val parts = text.split(':')
        if (parts.size != 2) return ParseResult.Invalid(TimeProblem.MALFORMED)
        val hour = parts[0].toIntOrNull() ?: return ParseResult.Invalid(TimeProblem.MALFORMED)
        val minute = parts[1].toIntOrNull() ?: return ParseResult.Invalid(TimeProblem.MALFORMED)
        if (parts[0].length !in 1..2 || parts[1].length != 2) {
            return ParseResult.Invalid(TimeProblem.MALFORMED)
        }
        if (hour !in 0..23) return ParseResult.Invalid(TimeProblem.HOUR_OUT_OF_RANGE)
        if (minute !in 0..59) return ParseResult.Invalid(TimeProblem.MINUTE_OUT_OF_RANGE)
        return ParseResult.Valid(TimeResult(hour, minute))
    }

    fun parseDate(raw: String): ParseResult<LocalDate> {
        val text = raw.trim()
        if (text.isEmpty()) return ParseResult.Invalid(TimeProblem.EMPTY)
        val date = try {
            LocalDate.parse(text, dateFormatter)
        } catch (_: DateTimeException) {
            return ParseResult.Invalid(TimeProblem.DATE_MALFORMED)
        }
        return ParseResult.Valid(date)
    }

    fun parseDateInFuture(raw: String, today: LocalDate): ParseResult<LocalDate> {
        return when (val result = parseDate(raw)) {
            is ParseResult.Valid -> {
                if (result.value.isBefore(today)) {
                    ParseResult.Invalid(TimeProblem.DATE_IN_PAST)
                } else {
                    result
                }
            }
            is ParseResult.Invalid -> result
        }
    }

    fun parseZone(raw: String): ParseResult<ZoneId> {
        val text = raw.trim()
        if (text.isEmpty()) return ParseResult.Invalid(TimeProblem.EMPTY)
        if (text.contains(' ') || text.any { it.isISOControl() }) {
            return ParseResult.Invalid(TimeProblem.UNKNOWN_TIMEZONE)
        }
        return try {
            ParseResult.Valid(ZoneId.of(text))
        } catch (_: DateTimeException) {
            ParseResult.Invalid(TimeProblem.UNKNOWN_TIMEZONE)
        }
    }

    fun formatTime(hour: Int, minute: Int): String = String.format(Locale.ROOT, "%02d:%02d", hour, minute)
}

/** ISO weekday helpers: 1 = Monday ... 7 = Sunday. */
object Weekdays {
    val ALL: List<Int> = (1..7).toList()
    val MONDAY_TO_FRIDAY: List<Int> = listOf(1, 2, 3, 4, 5)
    val WEEKEND: List<Int> = listOf(6, 7)

    fun normalize(days: Iterable<Int>): List<Int> = days
        .filter { it in 1..7 }
        .distinct()
        .sorted()

    fun isValid(days: Iterable<Int>): Boolean {
        val list = days.toList()
        return list.isNotEmpty() && list.all { it in 1..7 } && list.distinct().size == list.size
    }

    /** Parses strict `1,3,5` / `1 3 5` input. Returns null for invalid input. */
    fun parse(raw: String): List<Int>? {
        val tokens = raw.trim().split(',', ' ')
            .map { it.trim() }
            .filter { it.isNotEmpty() }
        if (tokens.isEmpty()) return null
        val parsed = tokens.map { it.toIntOrNull() ?: return null }
        if (parsed.any { it !in 1..7 }) return null
        return normalize(parsed)
    }

    fun toIsoString(days: Iterable<Int>): String = normalize(days).joinToString(",")
}

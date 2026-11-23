package dev.proflyder.home.core.time

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

interface DueTextStrings {
    fun today(): String

    fun tomorrow(): String

    fun dayOfWeek(isoDay: Int): String

    fun time(hour: Int, minute: Int): String = String.format(Locale.ROOT, "%02d:%02d", hour, minute)
}

object DueText {
    private const val WEEK_DAYS = 7L

    fun format(epochSeconds: Long, zone: ZoneId, nowUnix: Long, strings: DueTextStrings): String {
        val due = Instant.ofEpochSecond(epochSeconds).atZone(zone).toLocalDateTime()
        val today = Instant.ofEpochSecond(nowUnix).atZone(zone).toLocalDate()
        val time = strings.time(due.hour, due.minute)
        val date = due.toLocalDate()
        return when {
            date == today -> "${strings.today()} $time"
            date == today.plusDays(1) -> "${strings.tomorrow()} $time"
            date.isBefore(today.plusDays(WEEK_DAYS)) -> "${strings.dayOfWeek(due.dayOfWeek.value)} $time"
            else -> "${date.format(DateTimeFormatter.ofPattern("d MMM", Locale.getDefault()))} $time"
        }
    }
}

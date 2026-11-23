package dev.proflyder.home.feature.settings.presentation

import dev.proflyder.home.core.model.NotificationHistory
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

internal enum class HistoryDay { TODAY, YESTERDAY, EARLIER }

internal sealed interface HistoryRow {
    val key: String

    data class Day(val date: LocalDate, val day: HistoryDay) : HistoryRow {
        override val key: String get() = "day:$date"
    }

    data class Item(val entry: NotificationHistory) : HistoryRow {
        override val key: String get() = "entry:${entry.id}"
    }
}

internal object HistoryRows {
    fun of(entries: List<NotificationHistory>, today: LocalDate, zone: ZoneId): List<HistoryRow> =
        entries.groupBy { dateOf(it, zone) }.flatMap { (date, items) -> listOf(day(date, today)) + items.map(HistoryRow::Item) }

    private fun dateOf(entry: NotificationHistory, zone: ZoneId): LocalDate =
        Instant.ofEpochSecond(entry.createdUnix).atZone(zone).toLocalDate()

    private fun day(date: LocalDate, today: LocalDate): HistoryRow.Day = HistoryRow.Day(
        date,
        when (date) {
            today -> HistoryDay.TODAY
            today.minusDays(1) -> HistoryDay.YESTERDAY
            else -> HistoryDay.EARLIER
        },
    )
}

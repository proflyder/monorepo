package dev.proflyder.home.feature.timers

import dev.proflyder.home.core.model.Timer
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes

val TIMER_PRESETS: List<Duration> = listOf(1.minutes, 5.minutes, 10.minutes, 25.minutes)

val TimerOrder: Comparator<Timer> = compareBy({ it.createdUnix }, { it.id })

fun Duration.toTimerInput(): String = toComponents { hours, minutes, seconds, _ ->
    listOfNotNull(
        "${hours}h".takeIf { hours > 0 },
        "${minutes}m".takeIf { minutes > 0 },
        "${seconds}s".takeIf { seconds > 0 },
    ).joinToString(" ").ifEmpty { "0s" }
}

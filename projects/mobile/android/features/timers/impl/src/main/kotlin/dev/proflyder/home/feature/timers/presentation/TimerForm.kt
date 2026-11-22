package dev.proflyder.home.feature.timers.presentation

import dev.proflyder.home.core.time.DurationText

internal data class TimerForm(
    val label: String = "",
    val duration: String = "",
    val durationInvalid: Boolean = false,
) {
    val seconds: Long? get() = DurationText.parseOrNull(duration)

    fun labelOr(default: String): String = label.trim().ifBlank { default }

    fun validated(): TimerForm = copy(durationInvalid = seconds == null)
}

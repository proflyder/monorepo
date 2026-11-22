package dev.proflyder.home.feature.control.presentation

import android.content.Context
import dev.proflyder.home.feature.control.WakeFeedbackKind
import dev.proflyder.home.feature.control.impl.R

internal fun ControlNotice.text(context: Context): String = when (this) {
    is ControlNotice.ScenarioQueued -> context.getString(R.string.control_scenario_queued, title)
    is ControlNotice.FavoriteChanged -> context.getString(
        if (pinned) R.string.control_favorite_pinned else R.string.control_favorite_unpinned,
        subject.title(context),
    )
    is ControlNotice.WakeMarked -> context.getString(
        when (kind) {
            WakeFeedbackKind.FALSE_ALARM -> R.string.control_wake_false_alarm_sent
            WakeFeedbackKind.MISSED -> R.string.control_wake_missed_sent
        },
    )
    ControlNotice.CommandSent -> context.getString(R.string.control_command_sent)
}

internal fun PinSubject.title(context: Context): String = when (this) {
    is PinSubject.Entry -> entry.localizedDisplayName(context)
    is PinSubject.ScenarioPin -> scenario.label
}

package dev.proflyder.home.feature.reminders

import dev.proflyder.home.core.model.Schedule

val ReminderOrder: Comparator<Schedule> =
    compareBy({ it.onceDate }, { it.at.hour }, { it.at.minute }, { it.label })

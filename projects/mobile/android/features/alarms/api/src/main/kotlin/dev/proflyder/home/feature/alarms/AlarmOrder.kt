package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.Schedule

val AlarmOrder: Comparator<Schedule> = compareBy({ it.at.hour }, { it.at.minute }, { it.label })

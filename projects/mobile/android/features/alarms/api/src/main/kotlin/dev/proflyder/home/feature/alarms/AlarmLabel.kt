package dev.proflyder.home.feature.alarms

fun AlarmDraft.withDefaultLabel(defaultLabel: String): AlarmDraft =
    if (label.isBlank()) copy(label = defaultLabel) else this

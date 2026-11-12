package dev.proflyder.home.feature.alarms

/**
 * The alarm name is optional in the editor. A blank or whitespace-only name
 * keeps the localized default, so the server always receives a non-empty label.
 * The API stays resource-free; the caller supplies the string from `values/`.
 */
fun AlarmDraft.withDefaultLabel(defaultLabel: String): AlarmDraft =
    if (label.isBlank()) copy(label = defaultLabel) else this

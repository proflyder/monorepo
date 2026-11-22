package dev.proflyder.home.feature.time

enum class TimeSection(val featureId: String) {
    ALARMS("alarms"),
    TIMERS("timers"),
    REMINDERS("reminders"),
    ;

    companion object {
        fun fromFeatureId(value: String?): TimeSection? = entries.firstOrNull { it.featureId == value }
    }
}

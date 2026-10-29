package dev.proflyder.home.feature.control

/** What the Home screen shows; chosen with the chips above the list. */
sealed interface ControlFilter {
    data object All : ControlFilter
    data object Scenarios : ControlFilter
    data object Lights : ControlFilter
    data object Switches : ControlFilter
    data class Room(val name: String) : ControlFilter

    companion object {
        private const val ROOM_PREFIX = "room:"

        /** Stable text form used for persistence. */
        fun encode(filter: ControlFilter): String = when (filter) {
            All -> "all"
            Scenarios -> "scenarios"
            Lights -> "lights"
            Switches -> "switches"
            is Room -> ROOM_PREFIX + filter.name
        }

        /** Inverse of [encode]; null for unknown or malformed values. */
        fun decode(value: String?): ControlFilter? = when {
            value == null -> null
            value == "all" -> All
            value == "scenarios" -> Scenarios
            value == "lights" -> Lights
            value == "switches" -> Switches
            value.startsWith(ROOM_PREFIX) -> value.removePrefix(ROOM_PREFIX).takeIf { it.isNotBlank() }?.let(::Room)
            else -> null
        }
    }
}

/** Remembers UI choices of the Home screen across app restarts. */
interface ControlPreferences {
    /** The last selected filter, or null when none was saved (or it cannot be read). */
    suspend fun loadFilter(): ControlFilter?

    suspend fun saveFilter(filter: ControlFilter)
}

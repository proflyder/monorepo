package dev.proflyder.home.feature.control

sealed interface ControlFilter {
    data object All : ControlFilter
    data object Scenarios : ControlFilter
    data object Lights : ControlFilter
    data object Switches : ControlFilter
    data class Room(val name: String) : ControlFilter

    companion object {
        private const val ROOM_PREFIX = "room:"

        fun encode(filter: ControlFilter): String = when (filter) {
            All -> "all"
            Scenarios -> "scenarios"
            Lights -> "lights"
            Switches -> "switches"
            is Room -> ROOM_PREFIX + filter.name
        }

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

interface ControlPreferences {
    suspend fun loadFilter(): ControlFilter?

    suspend fun saveFilter(filter: ControlFilter)
}

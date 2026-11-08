package dev.proflyder.home

import android.os.Bundle
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.alarms.presentation.AlarmsFragment
import dev.proflyder.home.feature.reminders.presentation.RemindersFragment
import dev.proflyder.home.feature.timers.presentation.TimersFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

/** Sections of the single "Time" bottom tab. */
enum class TimeSection(val routeSegment: String) {
    ALARMS("alarms"),
    TIMERS("timers"),
    REMINDERS("reminders"),
    ;

    companion object {
        fun fromRouteSegment(value: String?): TimeSection? =
            entries.firstOrNull { it.routeSegment == value }
    }
}

/**
 * The "Time" tab. Each section is one route so a deep link
 * (`router.selectTab("alarms")`) can open the tab at the right switch position,
 * while the in-screen toggle changes sections without creating back entries.
 */
data class TimeDestination(val section: TimeSection = TimeSection.ALARMS) : Destination {
    override val featureId: String = "time"
    override val route: String = "time/${section.routeSegment}"

    override fun arguments(): Bundle = Bundle().apply {
        putString(ARG_SECTION, section.routeSegment)
    }

    companion object {
        const val ARG_SECTION = "time_section"
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class TimeFeatureEntry : FeatureEntry {
    override val id: String = "time"
    override val order: Int = 2
    override val titleRes: Int = R.string.app_time_title
    override val iconRes: Int = R.drawable.app_ic_time
    override val rootDestination: Destination = TimeDestination(TimeSection.ALARMS)

    override fun ownsRoute(route: String): Boolean = route.startsWith("$id/")

    override fun supports(destination: Destination): Boolean = destination is TimeDestination

    override fun createFragment(destination: Destination): Fragment = TimeFragment()

    override fun hostedDestination(featureId: String): Destination? = when (featureId) {
        "alarms" -> TimeDestination(TimeSection.ALARMS)
        "timers" -> TimeDestination(TimeSection.TIMERS)
        "reminders" -> TimeDestination(TimeSection.REMINDERS)
        else -> null
    }
}

/** Placeholder kept next to the entry so the hosting rule is obvious in one file. */
internal fun sectionFragment(section: TimeSection): Fragment = when (section) {
    TimeSection.ALARMS -> AlarmsFragment()
    TimeSection.TIMERS -> TimersFragment()
    TimeSection.REMINDERS -> RemindersFragment()
}

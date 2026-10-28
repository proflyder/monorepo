package dev.proflyder.home.feature.timers

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.timers.impl.R
import dev.proflyder.home.feature.timers.presentation.TimersFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object TimerListDestination : Destination {
    override val featureId: String = "timers"
    override val route: String = "timers/list"
}

@ContributesIntoSet(AppScope::class)
@Inject
class TimersFeatureEntry : FeatureEntry {
    override val id: String = "timers"
    override val order: Int = 1
    override val titleRes: Int = R.string.timers_title
    override val iconRes: Int = R.drawable.timers_ic_tab
    override val rootDestination: Destination = TimerListDestination

    override fun supports(destination: Destination): Boolean = destination is TimerListDestination

    override fun createFragment(destination: Destination): Fragment = TimersFragment()
}

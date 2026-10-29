package dev.proflyder.home.feature.control

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.presentation.ControlFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object ControlListDestination : Destination {
    override val featureId: String = "control"
    override val route: String = "control/list"
}

@ContributesIntoSet(AppScope::class)
@Inject
class ControlFeatureEntry : FeatureEntry {
    override val id: String = "control"
    override val order: Int = 3
    override val titleRes: Int = R.string.control_title
    override val iconRes: Int = R.drawable.control_ic_tab
    override val rootDestination: Destination = ControlListDestination

    override fun supports(destination: Destination): Boolean = destination is ControlListDestination

    override fun createFragment(destination: Destination): Fragment = ControlFragment()
}

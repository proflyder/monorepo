package dev.proflyder.home.feature.push

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.presentation.PushSettingsFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object PushSettingsDestination : Destination {
    override val featureId: String = "push"
    override val route: String = "push/settings"
}

@ContributesIntoSet(AppScope::class)
@Inject
class PushFeatureEntry : FeatureEntry {
    override val id: String = "push"
    override val order: Int = 6
    override val isTab: Boolean = false
    override val titleRes: Int = R.string.push_title
    override val iconRes: Int = R.drawable.push_ic_notification
    override val rootDestination = PushSettingsDestination

    override fun createFragment(destination: Destination): Fragment = PushSettingsFragment()
}

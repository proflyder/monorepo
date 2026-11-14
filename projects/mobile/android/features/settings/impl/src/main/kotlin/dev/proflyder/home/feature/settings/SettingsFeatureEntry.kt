package dev.proflyder.home.feature.settings

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.presentation.NotificationHistoryFragment
import dev.proflyder.home.feature.settings.presentation.SettingsFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

/** The single settings list the top app bar opens. */
object SettingsDestination : Destination {
    override val featureId: String = "settings"
    override val route: String = "settings/hub"
}

/** The local log of received notifications, opened from the settings list. */
object NotificationHistoryDestination : Destination {
    override val featureId: String = "settings"
    override val route: String = "settings/history"
}

/**
 * Settings is not a bottom tab: it opens as an overlay from the top app bar and
 * pushes its own sub-screens (the history) on the same stack, so Back returns
 * to the list and then to the tab the owner came from.
 */
@ContributesIntoSet(AppScope::class)
@Inject
class SettingsFeatureEntry : FeatureEntry {
    override val id: String = "settings"
    override val order: Int = 7
    override val isTab: Boolean = false
    override val titleRes: Int = R.string.settings_title
    override val iconRes: Int = R.drawable.settings_ic_tab
    override val rootDestination: Destination = SettingsDestination

    override fun titleRes(destination: Destination): Int = when (destination) {
        NotificationHistoryDestination -> R.string.settings_history_title
        else -> titleRes
    }

    override fun supports(destination: Destination): Boolean =
        destination is SettingsDestination || destination is NotificationHistoryDestination

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        NotificationHistoryDestination -> NotificationHistoryFragment()
        else -> SettingsFragment()
    }
}

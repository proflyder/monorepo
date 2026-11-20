package dev.proflyder.home.feature.settings

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.presentation.AppearanceFragment
import dev.proflyder.home.feature.settings.presentation.NotificationHistoryFragment
import dev.proflyder.home.feature.settings.presentation.SettingsFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object SettingsDestination : Destination {
    override val featureId: String = "settings"
    override val route: String = "settings/hub"
}

object NotificationHistoryDestination : Destination {
    override val featureId: String = "settings"
    override val route: String = "settings/history"
}

object AppearanceDestination : Destination {
    override val featureId: String = "settings"
    override val route: String = "settings/appearance"
}

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
        AppearanceDestination -> R.string.settings_appearance_title
        else -> titleRes
    }

    override fun supports(destination: Destination): Boolean =
        destination is SettingsDestination ||
            destination is NotificationHistoryDestination ||
            destination is AppearanceDestination

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        NotificationHistoryDestination -> NotificationHistoryFragment()
        AppearanceDestination -> AppearanceFragment()
        else -> SettingsFragment()
    }
}

package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.feature.settings.NotificationHistoryDestination
import dev.proflyder.home.feature.settings.impl.databinding.FragmentSettingsBinding

/**
 * One settings list: server connection, phone notifications and the notification
 * history. The first two rows open the existing feature screens as overlays;
 * the history is a screen of this feature.
 *
 * Rows are feature ids, not destination objects, so this aggregator does not
 * depend on the implementation modules it links to.
 */
class SettingsFragment : BaseFragment<FragmentSettingsBinding>(FragmentSettingsBinding.Companion::inflate) {
    override fun onBindingCreated(binding: FragmentSettingsBinding, savedInstanceState: Bundle?) {
        binding.settingsConnection.setOnClickListener { openFeature(CONNECTION_FEATURE) }
        binding.settingsNotifications.setOnClickListener { openFeature(PUSH_FEATURE) }
        binding.settingsHistory.setOnClickListener { requireNavigator().navigate(NotificationHistoryDestination) }
    }

    private fun openFeature(featureId: String) = requireNavigator().selectTab(featureId)

    private companion object {
        const val CONNECTION_FEATURE = "connection"
        const val PUSH_FEATURE = "push"
    }
}

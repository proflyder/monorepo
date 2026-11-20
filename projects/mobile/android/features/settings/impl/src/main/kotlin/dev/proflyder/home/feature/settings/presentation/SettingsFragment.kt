package dev.proflyder.home.feature.settings.presentation

import android.content.pm.ApplicationInfo
import android.os.Bundle
import androidx.core.view.isVisible
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.feature.settings.AppearanceDestination
import dev.proflyder.home.feature.settings.DesignSystemDestination
import dev.proflyder.home.feature.settings.NotificationHistoryDestination
import dev.proflyder.home.feature.settings.impl.databinding.FragmentSettingsBinding

class SettingsFragment : BaseFragment<FragmentSettingsBinding>(FragmentSettingsBinding.Companion::inflate) {
    override fun onBindingCreated(binding: FragmentSettingsBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
        binding.settingsConnection.setOnClickListener { openFeature(CONNECTION_FEATURE) }
        binding.settingsNotifications.setOnClickListener { openFeature(PUSH_FEATURE) }
        binding.settingsHistory.setOnClickListener { requireNavigator().navigate(NotificationHistoryDestination) }
        binding.settingsAppearance.setOnClickListener { requireNavigator().navigate(AppearanceDestination) }
        binding.settingsDesignSystem.isVisible = isDebugBuild()
        binding.settingsDesignSystem.setOnClickListener { requireNavigator().navigate(DesignSystemDestination) }
    }

    private fun isDebugBuild(): Boolean = requireContext().applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0

    private fun openFeature(featureId: String) = requireNavigator().selectTab(featureId)

    private companion object {
        const val CONNECTION_FEATURE = "connection"
        const val PUSH_FEATURE = "push"
    }
}

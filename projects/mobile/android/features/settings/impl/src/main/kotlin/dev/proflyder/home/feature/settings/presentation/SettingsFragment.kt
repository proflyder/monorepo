package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import android.view.View
import androidx.core.view.isVisible
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.feature.settings.impl.databinding.FragmentSettingsBinding

internal class SettingsFragment :
    ScreenFragment<FragmentSettingsBinding, SettingsUiState, SettingsIntent, SettingsEvent>(
        FragmentSettingsBinding.Companion::inflate,
    ) {
    override val viewModel: SettingsViewModel by screenViewModel()

    override fun onViewReady(binding: FragmentSettingsBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
        link(binding.settingsConnection, SettingsLink.CONNECTION)
        link(binding.settingsVpn, SettingsLink.VPN)
        link(binding.settingsNotifications, SettingsLink.NOTIFICATIONS)
        link(binding.settingsAppearance, SettingsLink.APPEARANCE)
        link(binding.settingsDesignSystem, SettingsLink.DESIGN_SYSTEM)
        render(SettingsUiState::designSystemVisible) { binding.settingsDesignSystem.isVisible = it }
        render(SettingsUiState::links) { renderLinks(it) }
    }

    override fun onEvent(event: SettingsEvent) {
        when (event) {
            is SettingsEvent.OpenRoute -> navigate(event.route)
        }
    }

    private fun renderLinks(links: Set<SettingsLink>) {
        binding.settingsConnection.isVisible = SettingsLink.CONNECTION in links
        binding.settingsVpn.isVisible = SettingsLink.VPN in links
        binding.settingsNotifications.isVisible = SettingsLink.NOTIFICATIONS in links
        binding.settingsAppearance.isVisible = SettingsLink.APPEARANCE in links
    }

    private fun link(view: View, link: SettingsLink) {
        view.setOnClickListener { dispatch(SettingsIntent.Open(link)) }
    }
}

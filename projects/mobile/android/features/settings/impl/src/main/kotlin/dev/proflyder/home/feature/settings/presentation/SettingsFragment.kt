package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import android.view.View
import androidx.core.view.isVisible
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.feature.settings.impl.R
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
        binding.settingsUpdates.setOnClickListener { dispatch(SettingsIntent.CheckUpdates) }
        render(SettingsUiState::designSystemVisible) { binding.settingsDesignSystem.isVisible = it }
        render(SettingsUiState::links) { renderLinks(it) }
        render(SettingsUiState::update, ::renderUpdate)
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

    private fun renderUpdate(row: SettingsUpdateRow) {
        binding.settingsUpdates.isVisible = row.visible
        binding.settingsSectionSystem.isVisible = row.visible
        binding.settingsUpdatesSubtitle.text = updateSubtitle(row)
    }

    private fun updateSubtitle(row: SettingsUpdateRow): String = when (row.status) {
        SettingsUpdateStatus.IDLE -> getString(R.string.settings_updates_version, row.version)
        SettingsUpdateStatus.CHECKING -> getString(R.string.settings_updates_checking, row.version)
        SettingsUpdateStatus.UP_TO_DATE -> getString(R.string.settings_updates_up_to_date, row.version)
        SettingsUpdateStatus.AVAILABLE -> getString(R.string.settings_updates_available, row.latestVersion)
        SettingsUpdateStatus.SIGN_IN_REQUIRED -> getString(R.string.settings_updates_sign_in)
        SettingsUpdateStatus.DOWNLOADING -> getString(R.string.settings_updates_downloading)
        SettingsUpdateStatus.INSTALLING -> getString(R.string.settings_updates_installing)
        SettingsUpdateStatus.OFFLINE -> getString(R.string.settings_updates_offline)
        SettingsUpdateStatus.FAILED -> getString(R.string.settings_updates_failed)
    }

    private fun link(view: View, link: SettingsLink) {
        view.setOnClickListener { dispatch(SettingsIntent.Open(link)) }
    }
}

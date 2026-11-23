package dev.proflyder.home.feature.push.presentation

import dev.proflyder.home.core.presentation.bindText
import android.os.Bundle
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.designsystem.ListRow
import dev.proflyder.home.core.designsystem.Tone
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.push.PushPermissionStatus
import dev.proflyder.home.feature.push.PushRegistration
import dev.proflyder.home.feature.push.PushRegistrationResult
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.impl.databinding.FragmentPushSettingsBinding

internal class PushSettingsFragment :
    ScreenFragment<FragmentPushSettingsBinding, PushSettingsUiState, PushSettingsIntent, PushSettingsEvent>(
        FragmentPushSettingsBinding.Companion::inflate,
    ) {
    override val viewModel: PushSettingsViewModel by screenViewModel()

    override fun onViewReady(binding: FragmentPushSettingsBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
        binding.root.keepFocusedVisible()
        bindRow(binding.pushNotificationsRow, binding.pushNotificationsAction, PushSettingsScreen.NOTIFICATIONS)
        bindRow(binding.pushDndRow, binding.pushDndAction, PushSettingsScreen.DND)
        bindRow(binding.pushFullScreenRow, binding.pushFullScreenAction, PushSettingsScreen.FULL_SCREEN)
        binding.pushRegister.setOnClickListener { dispatch(PushSettingsIntent.Register) }
        binding.pushDeviceNameSave.setOnClickListener { dispatch(PushSettingsIntent.SaveName) }
        val nameBinder = bindText(binding.pushDeviceName, PushSettingsIntent::EditName)
        render(PushSettingsUiState::permissions, ::renderPermissions)
        render({ it.registration to it.sync.busy }) { (registration, busy) -> renderRegistration(registration, busy) }
        render(PushSettingsUiState::deviceName, nameBinder::render)
    }

    private fun renderPermissions(permissions: PushPermissionStatus) = with(binding) {
        renderRow(pushNotificationsRow, permissions.notifications)
        renderRow(pushDndRow, permissions.dndAccess)
        renderRow(pushFullScreenRow, permissions.fullScreenIntent)
        pushRingSummary.setText(if (permissions.canRing) R.string.push_ring_ready else R.string.push_ring_incomplete)
    }

    private fun renderRegistration(registration: PushRegistration, busy: Boolean) = with(binding) {
        pushRegistration.text = registrationText(registration, busy)
        pushRegister.isEnabled = !busy
        pushDeviceNameSave.isEnabled = !busy
        pushProgress.setVisible(busy)
    }

    override fun onEvent(event: PushSettingsEvent) {
        when (event) {
            PushSettingsEvent.Registered -> showMessage(R.string.push_registered)
            PushSettingsEvent.NameSaved -> showMessage(R.string.push_device_name_saved)
            is PushSettingsEvent.Failed -> showError(registrationError(event.result))
            is PushSettingsEvent.OpenSettings -> open(event.screen)
        }
    }

    private fun registrationError(result: PushRegistrationResult): String = when (result) {
        PushRegistrationResult.NoPushToken -> getString(R.string.push_register_failed_no_token)
        PushRegistrationResult.NotConfigured -> getString(R.string.push_register_failed_not_configured)
        is PushRegistrationResult.Rejected -> getString(R.string.push_register_failed_reason, result.failure.userMessage(requireContext()))
        PushRegistrationResult.Done -> getString(R.string.push_registered)
    }

    private fun bindRow(row: ListRow, action: MaterialButton, screen: PushSettingsScreen) {
        action.contentDescription = getString(R.string.push_permission_open_settings, row.title)
        row.setOnClickListener { dispatch(PushSettingsIntent.OpenSettings(screen)) }
        action.setOnClickListener { dispatch(PushSettingsIntent.OpenSettings(screen)) }
    }

    private fun renderRow(row: ListRow, granted: Boolean) {
        row.subtitle = getString(if (granted) R.string.push_permission_granted else R.string.push_permission_missing)
        row.subtitleTone = if (granted) Tone.NEUTRAL else Tone.ERROR
    }

    private fun registrationText(registration: PushRegistration, busy: Boolean): String = when {
        busy -> getString(R.string.push_registering)
        registration.token == null -> getString(R.string.push_not_registered)
        else -> getString(R.string.push_registered_hint, registration.tokenHint())
    }

    private fun open(screen: PushSettingsScreen) {
        runCatching { startActivity(screen.intent(requireContext())) }
            .onFailure { showError(R.string.push_settings_missing) }
    }
}

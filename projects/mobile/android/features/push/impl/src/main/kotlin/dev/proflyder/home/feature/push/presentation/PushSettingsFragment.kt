package dev.proflyder.home.feature.push.presentation

import android.os.Bundle
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.designsystem.ListRow
import dev.proflyder.home.core.designsystem.Tone
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.TextBinder
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.push.PushRegistration
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.impl.databinding.FragmentPushSettingsBinding

internal class PushSettingsFragment :
    ScreenFragment<FragmentPushSettingsBinding, PushSettingsUiState, PushSettingsIntent, PushSettingsEvent>(
        FragmentPushSettingsBinding.Companion::inflate,
    ) {
    override val viewModel: PushSettingsViewModel by screenViewModel()

    private lateinit var nameBinder: TextBinder

    override fun onScreenCreated(binding: FragmentPushSettingsBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
        binding.root.keepFocusedVisible()
        bindRow(binding.pushNotificationsRow, binding.pushNotificationsAction, PushSettingsScreen.NOTIFICATIONS)
        bindRow(binding.pushDndRow, binding.pushDndAction, PushSettingsScreen.DND)
        bindRow(binding.pushFullScreenRow, binding.pushFullScreenAction, PushSettingsScreen.FULL_SCREEN)
        binding.pushRegister.setOnClickListener { dispatch(PushSettingsIntent.Register) }
        binding.pushDeviceNameSave.setOnClickListener { dispatch(PushSettingsIntent.SaveName) }
        nameBinder = bindText(binding.pushDeviceName, PushSettingsIntent::EditName)
    }

    override fun render(state: PushSettingsUiState) {
        with(binding) {
            renderRow(pushNotificationsRow, state.permissions.notifications)
            renderRow(pushDndRow, state.permissions.dndAccess)
            renderRow(pushFullScreenRow, state.permissions.fullScreenIntent)
            pushRingSummary.setText(if (state.canRing) R.string.push_ring_ready else R.string.push_ring_incomplete)
            pushRegistration.text = registrationText(state.registration, state.sync.busy)
            pushRegister.isEnabled = !state.sync.busy
            pushDeviceNameSave.isEnabled = !state.sync.busy
            pushProgress.setVisible(state.sync.busy)
        }
        nameBinder.render(state.deviceName)
    }

    override fun onEvent(event: PushSettingsEvent) {
        when (event) {
            PushSettingsEvent.Registered -> showMessage(R.string.push_registered)
            PushSettingsEvent.NameSaved -> showMessage(R.string.push_device_name_saved)
            PushSettingsEvent.Failed -> showError(R.string.push_register_failed)
            is PushSettingsEvent.OpenSettings -> open(event.screen)
        }
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

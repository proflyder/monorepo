package dev.proflyder.home.feature.push.presentation

import android.content.ActivityNotFoundException
import android.os.Bundle
import androidx.fragment.app.viewModels
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.designsystem.ListRow
import dev.proflyder.home.core.designsystem.Tone
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.push.PushPermissions
import dev.proflyder.home.feature.push.PushRegistration
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.impl.databinding.FragmentPushSettingsBinding

class PushSettingsFragment :
    BaseFragment<FragmentPushSettingsBinding>(FragmentPushSettingsBinding.Companion::inflate) {
    private val viewModel: PushSettingsViewModel by viewModels {
        viewModelFactory(PushSettingsViewModel::class.java)
    }

    private val permissions by lazy { PushPermissions(requireContext().applicationContext) }

    override fun onBindingCreated(binding: FragmentPushSettingsBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
        binding.root.keepFocusedVisible()
        bindRow(binding.pushNotificationsRow, binding.pushNotificationsAction) { open(permissions.notifications()) }
        bindRow(binding.pushDndRow, binding.pushDndAction) { open(permissions.dnd()) }
        bindRow(binding.pushFullScreenRow, binding.pushFullScreenAction) { open(permissions.fullScreen()) }
        binding.pushRegister.setOnClickListener { viewModel.registerNow() }
        binding.pushDeviceNameSave.setOnClickListener {
            viewModel.saveDeviceName(binding.pushDeviceName.text.toString())
        }
        viewModel.state.observe { render(it) }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refreshPermissions()
    }

    private fun render(state: PushSettingsUiState) {
        with(binding) {
            renderRow(pushNotificationsRow, state.permissions.notifications)
            renderRow(pushDndRow, state.permissions.dndAccess)
            renderRow(pushFullScreenRow, state.permissions.fullScreenIntent)
            pushRingSummary.text = if (state.canRing) {
                getString(R.string.push_ring_ready)
            } else {
                getString(R.string.push_ring_incomplete)
            }
            pushRegistration.text = registrationText(state.registration)
            pushRegister.isEnabled = !state.registration.busy
            pushDeviceNameSave.isEnabled = !state.registration.busy
            pushProgress.setVisible(state.registration.busy)
            if (!pushDeviceName.hasFocus() && pushDeviceName.text.toString() != state.deviceName) {
                pushDeviceName.setText(state.deviceName)
            }
        }
        state.notice?.let { notice ->
            when (notice) {
                PushNotice.REGISTERED -> showMessage(R.string.push_registered)
                PushNotice.NAME_SAVED -> showMessage(R.string.push_device_name_saved)
                PushNotice.FAILED -> showError(R.string.push_register_failed)
            }
            viewModel.consumeNotice()
        }
    }

    private fun bindRow(row: ListRow, action: MaterialButton, open: () -> Unit) {
        action.contentDescription = getString(R.string.push_permission_open_settings, row.title)
        row.setOnClickListener { open() }
        action.setOnClickListener { open() }
    }

    private fun renderRow(row: ListRow, granted: Boolean) {
        row.subtitle = getString(permissionStatus(granted))
        row.subtitleTone = permissionTone(granted)
    }

    private fun permissionStatus(granted: Boolean): Int = when {
        granted -> R.string.push_permission_granted
        else -> R.string.push_permission_missing
    }

    private fun permissionTone(granted: Boolean): Tone = when {
        granted -> Tone.NEUTRAL
        else -> Tone.ERROR
    }

    private fun registrationText(registration: PushRegistration): String = when {
        registration.busy -> getString(R.string.push_registering)
        registration.token == null -> getString(R.string.push_not_registered)
        else -> getString(R.string.push_registered_hint, registration.tokenHint())
    }

    private fun open(intent: android.content.Intent) {
        runCatching { startActivity(intent) }
            .onFailure { failure ->
                if (failure is ActivityNotFoundException) showError(R.string.push_settings_missing)
            }
    }
}

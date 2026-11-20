package dev.proflyder.home.feature.push.presentation

import android.content.ActivityNotFoundException
import android.os.Bundle
import android.widget.TextView
import androidx.fragment.app.viewModels
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
        binding.pushNotificationsAction.setOnClickListener { open(permissions.notifications()) }
        binding.pushDndAction.setOnClickListener { open(permissions.dnd()) }
        binding.pushFullScreenAction.setOnClickListener { open(permissions.fullScreen()) }
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
            renderRow(
                title = pushNotificationsTitle,
                status = pushNotificationsStatus,
                action = pushNotificationsAction,
                granted = state.permissions.notifications,
            )
            renderRow(
                title = pushDndTitle,
                status = pushDndStatus,
                action = pushDndAction,
                granted = state.permissions.dndAccess,
            )
            renderRow(
                title = pushFullScreenTitle,
                status = pushFullScreenStatus,
                action = pushFullScreenAction,
                granted = state.permissions.fullScreenIntent,
            )
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

    private fun renderRow(title: TextView, status: TextView, action: android.widget.Button, granted: Boolean) {
        status.text = getString(
            if (granted) R.string.push_permission_granted else R.string.push_permission_missing,
        )
        action.text = getString(
            if (granted) R.string.push_permission_open else R.string.push_permission_grant,
        )
        title.alpha = if (granted) 1f else 0.85f
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

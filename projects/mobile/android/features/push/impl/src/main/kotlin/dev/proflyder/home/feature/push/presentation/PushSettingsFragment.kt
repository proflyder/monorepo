package dev.proflyder.home.feature.push.presentation

import android.content.ActivityNotFoundException
import android.os.Bundle
import android.widget.TextView
import androidx.fragment.app.viewModels
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.push.PushPermissions
import dev.proflyder.home.feature.push.PushRegistration
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.impl.databinding.FragmentPushSettingsBinding

/**
 * «Уведомления и поиск телефона»: the real state of the three permissions the
 * loud path needs and a button that opens the matching system page for each.
 *
 * Nothing is requested silently: a permission the owner has not granted is
 * shown as missing with an explicit action, because the find-phone ring can
 * only bypass silent mode and Do Not Disturb with notification-policy access
 * and can only take over the lock screen with the full-screen-intent allowance.
 */
class PushSettingsFragment :
    BaseFragment<FragmentPushSettingsBinding>(FragmentPushSettingsBinding.Companion::inflate) {
    private val viewModel: PushSettingsViewModel by viewModels {
        viewModelFactory(PushSettingsViewModel::class.java)
    }

    private val permissions by lazy { PushPermissions(requireContext().applicationContext) }

    override fun onBindingCreated(binding: FragmentPushSettingsBinding, savedInstanceState: Bundle?) {
        binding.pushNotificationsAction.setOnClickListener { open(permissions.notifications()) }
        binding.pushDndAction.setOnClickListener { open(permissions.dnd()) }
        binding.pushFullScreenAction.setOnClickListener { open(permissions.fullScreen()) }
        binding.pushRegister.setOnClickListener { viewModel.registerNow() }
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
            pushProgress.setVisible(state.registration.busy)
        }
        state.notice?.let { notice ->
            when (notice) {
                PushNotice.REGISTERED -> showMessage(R.string.push_registered)
                PushNotice.FAILED -> showError(R.string.push_register_failed)
            }
            viewModel.consumeNotice()
        }
    }

    private fun renderRow(title: TextView, status: TextView, action: android.widget.Button, granted: Boolean) {
        // The title already names the permission; the status is the state.
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

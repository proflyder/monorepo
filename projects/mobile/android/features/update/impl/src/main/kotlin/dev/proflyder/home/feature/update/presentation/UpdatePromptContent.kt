package dev.proflyder.home.feature.update.presentation

import android.content.Context
import androidx.annotation.StringRes
import androidx.core.view.isVisible
import dev.proflyder.home.feature.update.UpdateCenter
import dev.proflyder.home.feature.update.UpdateFailureKind
import dev.proflyder.home.feature.update.UpdatePhase
import dev.proflyder.home.feature.update.UpdateState
import dev.proflyder.home.feature.update.impl.R
import dev.proflyder.home.feature.update.impl.databinding.DialogUpdatePromptBinding

internal class UpdatePromptContent(
    private val context: Context,
    private val binding: DialogUpdatePromptBinding,
    private val updates: UpdateCenter,
) {
    init {
        binding.updateLater.setOnClickListener { updates.dismissPrompt() }
        binding.updateAction.setOnClickListener { performAction() }
    }

    fun render(state: UpdateState) {
        binding.updateTitle.setText(titleOf(state))
        binding.updateVersion.text = versionOf(state)
        renderMessage(state)
        renderNotes(state)
        renderProgress(state)
        renderActions(state)
    }

    private fun renderMessage(state: UpdateState) {
        val message = messageOf(state)
        binding.updateMessage.isVisible = message.isNotEmpty()
        binding.updateMessage.text = message
    }

    private fun renderNotes(state: UpdateState) {
        val notes = state.available?.releaseNotes.orEmpty().trim()
        binding.updateNotesTitle.isVisible = notes.isNotEmpty()
        binding.updateNotes.isVisible = notes.isNotEmpty()
        binding.updateNotes.text = notes
    }

    private fun renderProgress(state: UpdateState) {
        when (state.phase) {
            UpdatePhase.CHECKING -> showIndeterminate(R.string.home_update_progress_checking)
            UpdatePhase.SIGNING_IN -> showIndeterminate(R.string.home_update_progress_signing_in)
            UpdatePhase.DOWNLOADING -> showDownload(state.progressPercent)
            UpdatePhase.INSTALLING -> showStatus(R.string.home_update_progress_downloaded)
            else -> binding.updateProgress.isVisible = false
        }
    }

    private fun showStatus(@StringRes status: Int) {
        binding.updateProgress.isVisible = false
        binding.updateStatus.setText(status)
    }

    private fun renderActions(state: UpdateState) {
        val action = actionOf(state)
        binding.updateAction.isVisible = action != null
        action?.let(binding.updateAction::setText)
        binding.updateLater.isVisible = state.phase != UpdatePhase.DOWNLOADING
        binding.updateLater.setText(laterOf(state))
    }

    private fun showIndeterminate(@StringRes status: Int) {
        binding.updateProgress.isVisible = true
        binding.updateProgress.isIndeterminate = true
        binding.updateStatus.setText(status)
    }

    private fun showDownload(percent: Int?) {
        val progress = binding.updateProgress
        progress.isVisible = true
        progress.isIndeterminate = percent == null
        progress.setProgressCompat(percent ?: 0, false)
        binding.updateStatus.text = downloadStatus(percent)
    }

    private fun downloadStatus(percent: Int?): String =
        if (percent == null) {
            context.getString(R.string.home_update_progress_downloading_unknown)
        } else {
            context.getString(R.string.home_update_progress_downloading, percent)
        }

    @StringRes
    private fun titleOf(state: UpdateState): Int = when (state.phase) {
        UpdatePhase.CHECKING, UpdatePhase.SIGNING_IN -> R.string.home_update_title_checking
        UpdatePhase.UP_TO_DATE -> R.string.home_update_title_up_to_date
        UpdatePhase.SIGN_IN_REQUIRED -> R.string.home_update_title_sign_in
        UpdatePhase.FAILED -> R.string.home_update_title_failed
        else -> R.string.home_update_title_available
    }

    private fun versionOf(state: UpdateState): String {
        val available = state.available
        if (available != null) {
            return context.getString(R.string.home_update_version_available, available.versionName)
        }
        return context.getString(R.string.home_update_version_current, state.currentVersion)
    }

    private fun messageOf(state: UpdateState): String = when (state.phase) {
        UpdatePhase.SIGN_IN_REQUIRED -> signInMessage(state.failure)
        UpdatePhase.UP_TO_DATE -> context.getString(R.string.home_update_up_to_date)
        UpdatePhase.FAILED -> context.getString(errorOf(state.failure))
        else -> ""
    }

    private fun signInMessage(failure: UpdateFailureKind?): String =
        if (failure == UpdateFailureKind.SIGN_IN_CANCELLED) {
            context.getString(R.string.home_update_error_auth_cancelled)
        } else {
            context.getString(R.string.home_update_sign_in_explanation)
        }

    @StringRes
    private fun errorOf(failure: UpdateFailureKind?): Int = when (failure) {
        UpdateFailureKind.SIGN_IN_REQUIRED -> R.string.home_update_error_auth
        UpdateFailureKind.SIGN_IN_CANCELLED -> R.string.home_update_error_auth_cancelled
        UpdateFailureKind.OFFLINE -> R.string.home_update_error_offline
        UpdateFailureKind.DOWNLOAD -> R.string.home_update_error_download
        UpdateFailureKind.INSTALL -> R.string.home_update_error_install
        UpdateFailureKind.INSTALL_CANCELLED -> R.string.home_update_error_install_cancelled
        UpdateFailureKind.UNSUPPORTED -> R.string.home_update_error_unsupported
        else -> R.string.home_update_error_unknown
    }

    @StringRes
    private fun actionOf(state: UpdateState): Int? = when (state.phase) {
        UpdatePhase.AVAILABLE -> R.string.home_update_action_update
        UpdatePhase.SIGN_IN_REQUIRED -> R.string.home_update_action_sign_in
        UpdatePhase.FAILED -> R.string.home_update_action_retry
        else -> null
    }

    @StringRes
    private fun laterOf(state: UpdateState): Int =
        if (state.phase == UpdatePhase.UP_TO_DATE) R.string.home_update_action_close else R.string.home_update_action_later

    private fun performAction() {
        when (updates.state.value.phase) {
            UpdatePhase.AVAILABLE -> updates.install()
            UpdatePhase.SIGN_IN_REQUIRED -> updates.signIn()
            UpdatePhase.FAILED -> retry()
            else -> Unit
        }
    }

    private fun retry() {
        val failure = updates.state.value.failure
        val install = failure == UpdateFailureKind.DOWNLOAD || failure == UpdateFailureKind.INSTALL
        if (install) updates.install() else updates.checkNow()
    }
}

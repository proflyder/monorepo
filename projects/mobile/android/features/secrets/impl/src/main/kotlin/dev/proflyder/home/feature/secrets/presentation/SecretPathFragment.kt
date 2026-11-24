@file:Suppress("NoTry")

package dev.proflyder.home.feature.secrets.presentation

import android.content.Context
import android.os.Bundle
import android.text.format.DateFormat
import android.view.LayoutInflater
import androidx.appcompat.app.AlertDialog
import androidx.core.widget.doAfterTextChanged
import androidx.lifecycle.lifecycleScope
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.SecretVersion
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.navigation.destination
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.secrets.SecretsPathDestination
import dev.proflyder.home.feature.secrets.impl.R
import dev.proflyder.home.feature.secrets.impl.databinding.DialogSecretEditBinding
import dev.proflyder.home.feature.secrets.impl.databinding.DialogSecretStepupBinding
import dev.proflyder.home.feature.secrets.impl.databinding.FragmentSecretPathBinding
import java.time.Instant
import java.time.format.DateTimeParseException
import java.util.Date
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

internal class SecretPathFragment :
    SecretsScreenFragment<FragmentSecretPathBinding, SecretPathUiState, SecretPathIntent, SecretPathEvent>(
        FragmentSecretPathBinding.Companion::inflate,
    ) {
    override val viewModel: SecretPathViewModel by screenViewModel()

    private val route: SecretsPathDestination? by destination(SecretsPathDestination)

    private val signer by lazy { AndroidDeviceSigner(requireContext().applicationContext) }

    private val actions = object : SecretPathActions {
        override fun reveal(key: String) = dispatch(SecretPathIntent.Reveal(key))

        override fun copy(key: String) = dispatch(SecretPathIntent.Copy(key))

        override fun edit(key: String) = dispatch(SecretPathIntent.EditRequest(key))

        override fun rollback(version: Long) = dispatch(SecretPathIntent.RollbackRequest(version))

        override fun delete() = dispatch(SecretPathIntent.DeleteRequest)
    }

    private val adapter by lazy { SecretPathAdapter(actions) }
    private val clipboard by lazy { SecretClipboard(requireContext(), viewLifecycleOwner.lifecycleScope) }

    private var activeDialog: AlertDialog? = null
    private var activeTag: DialogTag? = null
    private var activeConfirm: SecretAction? = null
    private var stepUpBinding: DialogSecretStepupBinding? = null
    private var editBinding: DialogSecretEditBinding? = null
    private var notice: Job? = null

    override fun onScreenCreate(savedInstanceState: Bundle?) {
        super.onScreenCreate(savedInstanceState)
        route?.path?.let { dispatch(SecretPathIntent.SetPath(it)) }
    }

    override fun onViewReady(binding: FragmentSecretPathBinding, savedInstanceState: Bundle?) {
        binding.secretPathList.adapter = adapter
        binding.secretPathRefresh.setupPullToRefresh(binding.secretPathList) {
            dispatch(SecretPathIntent.Refresh)
        }
        binding.secretPathRetry.setOnClickListener { dispatch(SecretPathIntent.Refresh) }
        render(::rows) { rows -> adapter.submitList(rows) }
        render({ it.dialog }) { dialog -> renderDialog(dialog) }
        render(::status, ::renderStatus)
    }

    private fun rows(state: SecretPathUiState): List<SecretPathRow> {
        val meta = state.metadata ?: return emptyList()
        val keys = keyRows(meta.keys, state.revealedKey, state.revealedValue, meta.currentVersion, state.revealingKey)
        val versions = versionRows(
            versions = meta.versions,
            currentVersion = meta.currentVersion,
            createdLabel = ::createdLabel,
            rollbackLabel = getString(R.string.secrets_rollback),
            unavailableLabel = ::unavailableLabel,
        )
        return buildList {
            add(
                SecretPathRow.Header(
                    key = "header",
                    title = meta.path,
                    description = state.description,
                    currentVersion = meta.currentVersion,
                    sessionActive = state.sessionActive,
                ),
            )
            add(SecretPathRow.Note("keys:title", getString(R.string.secrets_keys)))
            if (keys.isEmpty()) add(SecretPathRow.Note("keys:empty", getString(R.string.secrets_keys_empty))) else addAll(keys)
            add(SecretPathRow.Note("versions:title", getString(R.string.secrets_versions)))
            addAll(versions)
        }
    }

    private fun status(state: SecretPathUiState): PathStatus = PathStatus(
        loading = state.loading,
        hasMetadata = state.metadata != null,
        failure = state.failure,
    )

    private fun renderStatus(status: PathStatus) = with(binding) {
        secretPathProgress.setVisible(status.loading && !status.hasMetadata)
        secretPathError.setVisible(!status.loading && !status.hasMetadata && status.failure != null)
        secretPathErrorText.text = status.failure?.userMessage(requireContext()).orEmpty()
        secretPathRefresh.isRefreshing = status.loading && status.hasMetadata
    }

    private fun renderDialog(dialog: SecretDialog?) {
        when (dialog) {
            null -> dismissDialog()
            is SecretDialog.StepUp -> renderStepUp(dialog)
            is SecretDialog.Edit -> renderEdit(dialog)
            is SecretDialog.Confirm -> renderConfirm(dialog)
        }
    }

    private fun dismissDialog() {
        stepUpBinding = null
        editBinding = null
        activeConfirm = null
        activeTag = null
        activeDialog?.setOnDismissListener(null)
        activeDialog?.setOnCancelListener(null)
        activeDialog?.dismiss()
        activeDialog = null
    }

    private fun renderStepUp(dialog: SecretDialog.StepUp) {
        val views = if (activeTag == DialogTag.STEP_UP) {
            checkNotNull(stepUpBinding)
        } else {
            createStepUp()
        }
        views.secretStepupProblem.text = dialog.problem?.let { secretProblemText(requireContext(), it) }.orEmpty()
        views.secretStepupProblem.setVisible(dialog.problem != null)
        views.secretStepupProgress.setVisible(dialog.busy)
        activeDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.isEnabled = !dialog.busy
    }

    private fun createStepUp(): DialogSecretStepupBinding {
        dismissDialog()
        val views = DialogSecretStepupBinding.inflate(LayoutInflater.from(requireContext()))
        val dialog = MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.secrets_stepup_title)
            .setView(views.root)
            .setPositiveButton(R.string.secrets_stepup_confirm, null)
            .setNegativeButton(R.string.secrets_cancel) { _, _ -> dispatch(SecretPathIntent.DialogDismiss) }
            .setOnCancelListener { dispatch(SecretPathIntent.DialogDismiss) }
            .create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                dispatch(SecretPathIntent.StepUpSubmit)
            }
        }
        dialog.show()
        activeDialog = dialog
        activeTag = DialogTag.STEP_UP
        stepUpBinding = views
        return views
    }

    private fun renderEdit(dialog: SecretDialog.Edit) {
        val views = if (activeTag == DialogTag.EDIT) {
            checkNotNull(editBinding)
        } else {
            createEdit(dialog.key)
        }
        if (views.secretEditInput.text?.toString().orEmpty() != dialog.value) {
            views.secretEditInput.setText(dialog.value)
        }
        views.secretEditMessage.text = dialog.problem?.let { secretProblemText(requireContext(), it) }.orEmpty()
        views.secretEditMessage.setVisible(dialog.problem != null)
        activeDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.isEnabled = !dialog.busy
    }

    private fun createEdit(key: String): DialogSecretEditBinding {
        dismissDialog()
        val views = DialogSecretEditBinding.inflate(LayoutInflater.from(requireContext()))
        views.secretEditInput.doAfterTextChanged { text ->
            dispatch(SecretPathIntent.EditValue(text?.toString().orEmpty()))
        }
        val dialog = MaterialAlertDialogBuilder(requireContext())
            .setTitle(getString(R.string.secrets_update_title, key))
            .setView(views.root)
            .setPositiveButton(R.string.secrets_continue, null)
            .setNegativeButton(R.string.secrets_cancel) { _, _ -> dispatch(SecretPathIntent.DialogDismiss) }
            .setOnCancelListener { dispatch(SecretPathIntent.DialogDismiss) }
            .create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                dispatch(SecretPathIntent.EditSubmit)
            }
        }
        dialog.show()
        activeDialog = dialog
        activeTag = DialogTag.EDIT
        editBinding = views
        return views
    }

    private fun renderConfirm(dialog: SecretDialog.Confirm) {
        if (activeTag == DialogTag.CONFIRM && activeConfirm == dialog.action) return
        dismissDialog()
        activeConfirm = dialog.action
        val alert = MaterialAlertDialogBuilder(requireContext())
            .setTitle(confirmTitle(dialog.action))
            .setMessage(confirmMessage(dialog.action))
            .setPositiveButton(R.string.secrets_continue) { _, _ -> dispatch(SecretPathIntent.ConfirmAccept) }
            .setNegativeButton(R.string.secrets_cancel) { _, _ -> dispatch(SecretPathIntent.DialogDismiss) }
            .setOnCancelListener { dispatch(SecretPathIntent.DialogDismiss) }
            .show()
        activeDialog = alert
        activeTag = DialogTag.CONFIRM
    }

    private fun confirmTitle(action: SecretAction): Int = when (action.kind) {
        SecretActionKind.ROLLBACK -> R.string.secrets_rollback_title
        else -> R.string.secrets_delete_title
    }

    private fun confirmMessage(action: SecretAction): String = when (action.kind) {
        SecretActionKind.ROLLBACK -> getString(R.string.secrets_rollback_message, action.version ?: 0)
        else -> getString(R.string.secrets_delete_message)
    }

    override fun onEvent(event: SecretPathEvent) {
        when (event) {
            is SecretPathEvent.CopyValue -> copy(event.value)
            is SecretPathEvent.Message -> showError(secretProblemText(requireContext(), event.problem))
            is SecretPathEvent.Sign -> sign(event)
            SecretPathEvent.Deleted -> {
                showMessage(R.string.secrets_deleted)
                navigate(Route.Back)
            }
        }
    }

    private fun sign(event: SecretPathEvent.Sign) {
        signer.requestSignature(requireActivity(), event.challenge.payload) { result ->
            when (result) {
                is DeviceSignatureResult.Signed -> dispatch(SecretPathIntent.StepUpSigned(result.signature))
                DeviceSignatureResult.Cancelled -> dispatch(SecretPathIntent.StepUpCancelled)
                is DeviceSignatureResult.Failed -> dispatch(SecretPathIntent.StepUpFailed(result.reason))
            }
        }
    }

    private fun copy(value: String) {
        clipboard.copy(value)
        notice?.cancel()
        notice = lifecycleScope.launch {
            delay(CLIPBOARD_NOTICE_MILLIS)
            showMessage(R.string.secrets_copied_cleared)
        }
        showMessage(R.string.secrets_copied)
    }

    private fun createdLabel(raw: String): String {
        if (raw.isBlank()) return ""
        val instant = try {
            Instant.parse(raw)
        } catch (error: DateTimeParseException) {
            return raw
        }
        val date = Date.from(instant)
        return DateFormat.getMediumDateFormat(requireContext()).format(date) +
            " " + DateFormat.getTimeFormat(requireContext()).format(date)
    }

    private fun unavailableLabel(version: SecretVersion): String? = when {
        version.destroyed -> getString(R.string.secrets_version_destroyed)
        !version.available -> getString(R.string.secrets_version_deleted)
        else -> null
    }

    override fun onDestroy() {
        notice?.cancel()
        signer.cancel()
        dismissDialog()
        super.onDestroy()
    }

    private companion object {
        const val CLIPBOARD_NOTICE_MILLIS = 30_000L
    }
}

internal data class PathStatus(
    val loading: Boolean,
    val hasMetadata: Boolean,
    val failure: dev.proflyder.home.core.model.RemoteFailure?,
)

internal enum class DialogTag { STEP_UP, EDIT, CONFIRM }

internal fun secretProblemText(context: Context, problem: SecretProblem): String = when (problem) {
    SecretProblem.StepUpRequired -> context.getString(R.string.secrets_problem_step_up)
    SecretProblem.InvalidSignature -> context.getString(R.string.secrets_problem_invalid_signature)
    SecretProblem.StepUpExpired -> context.getString(R.string.secrets_problem_step_up_expired)
    SecretProblem.DeviceNotEnrolled -> context.getString(R.string.secrets_problem_device_not_enrolled)
    SecretProblem.DeviceUnavailable -> context.getString(R.string.secrets_problem_device_unavailable)
    SecretProblem.EnrollmentRejected -> context.getString(R.string.secrets_problem_enrollment_rejected)
    SecretProblem.EnrollmentConflict -> context.getString(R.string.secrets_problem_enrollment_conflict)
    SecretProblem.VersionUnavailable -> context.getString(R.string.secrets_problem_version_unavailable)
    is SecretProblem.Conflict -> problem.currentVersion
        ?.let { context.getString(R.string.secrets_problem_conflict_version, it) }
        ?: context.getString(R.string.secrets_problem_conflict)
    is SecretProblem.RateLimited -> problem.retryAfterSeconds
        ?.let { context.resources.getQuantityString(R.plurals.secrets_problem_rate_limited_after, it.toInt(), it) }
        ?: context.getString(R.string.secrets_problem_rate_limited)
    SecretProblem.Forbidden -> context.getString(R.string.secrets_problem_forbidden)
    SecretProblem.KeyMissing -> context.getString(R.string.secrets_problem_key_missing)
    SecretProblem.NotFound -> context.getString(R.string.secrets_problem_not_found)
    is SecretProblem.Other -> problem.failure.userMessage(context)
}

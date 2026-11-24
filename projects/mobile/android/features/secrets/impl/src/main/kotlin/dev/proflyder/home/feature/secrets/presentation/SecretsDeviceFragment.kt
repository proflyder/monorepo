package dev.proflyder.home.feature.secrets.presentation

import android.os.Bundle
import android.text.format.DateFormat
import androidx.core.widget.doAfterTextChanged
import androidx.lifecycle.lifecycleScope
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.secrets.impl.R
import dev.proflyder.home.feature.secrets.impl.databinding.FragmentSecretsDeviceBinding
import java.util.Date
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal class SecretsDeviceFragment :
    SecretsScreenFragment<FragmentSecretsDeviceBinding, SecretsDeviceUiState, SecretsDeviceIntent, SecretsDeviceEvent>(
        FragmentSecretsDeviceBinding.Companion::inflate,
    ) {
    override val viewModel: SecretsDeviceViewModel by screenViewModel()

    private val signer by lazy { AndroidDeviceSigner(requireContext().applicationContext) }

    override fun onViewReady(binding: FragmentSecretsDeviceBinding, savedInstanceState: Bundle?) {
        binding.secretsDeviceCode.doAfterTextChanged { text ->
            dispatch(SecretsDeviceIntent.Code(text?.toString().orEmpty()))
        }
        binding.secretsDeviceBind.setOnClickListener { dispatch(SecretsDeviceIntent.Bind) }
        binding.secretsDeviceRebind.setOnClickListener { confirmRebind() }
        binding.secretsDeviceRetry.setOnClickListener { dispatch(SecretsDeviceIntent.Refresh) }
        if (!signer.isSupported()) {
            dispatch(SecretsDeviceIntent.KeyFailed(DeviceSignatureFailure.UNSUPPORTED))
        }
        render(::formStatus, ::renderForm)
        render(::bindStatus) { bind ->
            binding.secretsDeviceBind.isEnabled = bind.enabled
            binding.secretsDeviceCode.isEnabled = !bind.busy
            binding.secretsDeviceProgress.setVisible(bind.busy)
        }
        render({ it.problem }) { problem ->
            binding.secretsDeviceMessage.text = problem?.let { secretProblemText(requireContext(), it) }.orEmpty()
            binding.secretsDeviceMessage.setVisible(problem != null)
        }
    }

    private fun formStatus(state: SecretsDeviceUiState): DeviceStatus = DeviceStatus(
        loading = state.loading,
        enrolled = state.enrolled,
        rebinding = state.rebinding,
        deviceId = state.deviceId,
        enrolledAt = state.enrolledAt,
        failure = state.failure,
    )

    private fun bindStatus(state: SecretsDeviceUiState): BindStatus = BindStatus(
        enabled = state.keyReady && state.code.isNotBlank() && !state.busy &&
            (!state.enrolled || state.rebinding),
        busy = state.busy,
    )

    private fun renderForm(status: DeviceStatus) = with(binding) {
        secretsDeviceProgress.setVisible(status.loading)
        secretsDeviceError.setVisible(!status.loading && status.failure != null)
        secretsDeviceErrorText.text = status.failure?.userMessage(requireContext()).orEmpty()
        val setup = !status.loading && (!status.enrolled || status.rebinding)
        secretsDeviceForm.setVisible(!status.loading)
        secretsDeviceHelp.setVisible(setup)
        secretsDeviceCodeLayout.setVisible(setup)
        secretsDeviceBind.setVisible(setup)
        secretsDeviceRebind.setVisible(!status.loading && status.enrolled && !status.rebinding)
        secretsDeviceStatus.text = if (status.enrolled) {
            getString(R.string.secrets_device_enrolled, status.deviceId.orEmpty())
        } else {
            getString(R.string.secrets_device_not_enrolled)
        }
        secretsDeviceEnrolledAt.setVisible(status.enrolled && status.enrolledAt > 0)
        secretsDeviceEnrolledAt.text = if (status.enrolled && status.enrolledAt > 0) {
            getString(R.string.secrets_device_enrolled_at, enrolledDate(status.enrolledAt))
        } else {
            ""
        }
    }

    private fun enrolledDate(seconds: Long): String =
        DateFormat.getMediumDateFormat(requireContext()).format(Date(seconds * 1000))

    private fun confirmRebind() {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.secrets_device_rebind_title)
            .setMessage(R.string.secrets_device_rebind_message)
            .setPositiveButton(R.string.secrets_device_rebind) { _, _ ->
                dispatch(SecretsDeviceIntent.Rebind)
            }
            .setNegativeButton(R.string.secrets_cancel, null)
            .show()
    }

    override fun onEvent(event: SecretsDeviceEvent) {
        when (event) {
            is SecretsDeviceEvent.CreateKey -> createKey(event.challenge)
            is SecretsDeviceEvent.ReconcileKey -> signer.reconcileEnrollmentKey(event.serverPublicKey)
            SecretsDeviceEvent.Bound -> {
                signer.promoteEnrollmentKey()
                showMessage(R.string.secrets_device_bound)
                navigate(Route.Back)
            }
            is SecretsDeviceEvent.Problem -> showError(secretProblemText(requireContext(), event.problem))
        }
    }

    private fun createKey(challenge: String) {
        viewLifecycleOwner.lifecycleScope.launch {
            val result = withContext(Dispatchers.Default) { signer.createEnrollmentKey(challenge) }
            when (result) {
                is DeviceKeyResult.Ready -> dispatch(SecretsDeviceIntent.KeyReady(result.material))
                is DeviceKeyResult.Failed -> dispatch(SecretsDeviceIntent.KeyFailed(result.reason))
            }
        }
    }

    override fun onDestroy() {
        signer.cancel()
        super.onDestroy()
    }
}

internal data class DeviceStatus(
    val loading: Boolean,
    val enrolled: Boolean,
    val rebinding: Boolean,
    val deviceId: String?,
    val enrolledAt: Long,
    val failure: dev.proflyder.home.core.model.RemoteFailure?,
)

internal data class BindStatus(val enabled: Boolean, val busy: Boolean)

package dev.proflyder.home.feature.secrets.presentation

import android.os.Bundle
import androidx.core.widget.doAfterTextChanged
import androidx.lifecycle.lifecycleScope
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.secrets.impl.R
import dev.proflyder.home.feature.secrets.impl.databinding.FragmentSecretsDeviceBinding
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
        binding.secretsDeviceRetry.setOnClickListener { dispatch(SecretsDeviceIntent.Refresh) }
        if (!signer.isSupported()) {
            dispatch(SecretsDeviceIntent.KeyFailed(DeviceSignatureFailure.UNSUPPORTED))
        }
        render(::status, ::renderStatus)
        render({ it.enrolled }) { enrolled ->
            binding.secretsDeviceStatus.text = if (enrolled) {
                getString(R.string.secrets_device_enrolled, viewModel.state.value.deviceId.orEmpty())
            } else {
                getString(R.string.secrets_device_not_enrolled)
            }
        }
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

    private fun status(state: SecretsDeviceUiState): DeviceStatus = DeviceStatus(
        loading = state.loading,
        failure = state.failure,
    )

    private fun bindStatus(state: SecretsDeviceUiState): BindStatus = BindStatus(
        enabled = state.keyReady && state.code.isNotBlank() && !state.busy,
        busy = state.busy,
    )

    private fun renderStatus(status: DeviceStatus) = with(binding) {
        secretsDeviceProgress.setVisible(status.loading)
        secretsDeviceError.setVisible(!status.loading && status.failure != null)
        secretsDeviceErrorText.text = status.failure?.userMessage(requireContext()).orEmpty()
        secretsDeviceForm.setVisible(!status.loading)
    }

    override fun onEvent(event: SecretsDeviceEvent) {
        when (event) {
            is SecretsDeviceEvent.CreateKey -> createKey(event.challenge)
            SecretsDeviceEvent.Bound -> {
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
    val failure: dev.proflyder.home.core.model.RemoteFailure?,
)

internal data class BindStatus(val enabled: Boolean, val busy: Boolean)

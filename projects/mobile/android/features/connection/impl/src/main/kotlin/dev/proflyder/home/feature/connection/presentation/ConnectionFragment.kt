package dev.proflyder.home.feature.connection.presentation

import android.os.Bundle
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.viewModels
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.connection.impl.R
import dev.proflyder.home.feature.connection.impl.databinding.FragmentConnectionBinding
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

class ConnectionFragment :
    BaseFragment<FragmentConnectionBinding>(FragmentConnectionBinding.Companion::inflate) {
    private val viewModel: ConnectionViewModel by viewModels {
        viewModelFactory(ConnectionViewModel::class.java)
    }

    private var rendering = false

    override fun onBindingCreated(binding: FragmentConnectionBinding, savedInstanceState: Bundle?) {
        binding.connectionEndpoint.doAfterTextChanged {
            if (!rendering) viewModel.setEndpoint(it?.toString().orEmpty())
        }
        binding.connectionToken.doAfterTextChanged {
            if (!rendering) viewModel.setToken(it?.toString().orEmpty())
        }
        binding.connectionConnectTimeout.doAfterTextChanged {
            if (!rendering) viewModel.setConnectSeconds(it?.toString().orEmpty())
        }
        binding.connectionReadTimeout.doAfterTextChanged {
            if (!rendering) viewModel.setReadSeconds(it?.toString().orEmpty())
        }
        binding.connectionSave.setOnClickListener { viewModel.save() }
        binding.connectionTest.setOnClickListener { viewModel.test() }
        binding.connectionClearToken.setOnClickListener {
            binding.connectionToken.setText("")
            viewModel.requestClearToken()
        }

        viewModel.state.observe { render(it) }
    }

    private fun render(state: ConnectionUiState) {
        rendering = true
        with(binding) {
            if (connectionEndpoint.text?.toString() != state.endpointText) connectionEndpoint.setText(state.endpointText)
            if (connectionToken.text?.toString() != state.tokenInput) connectionToken.setText(state.tokenInput)
            if (connectionConnectTimeout.text?.toString() != state.connectSeconds) {
                connectionConnectTimeout.setText(state.connectSeconds)
            }
            if (connectionReadTimeout.text?.toString() != state.readSeconds) {
                connectionReadTimeout.setText(state.readSeconds)
            }

            connectionHttpWarning.setVisible(state.endpointText.trim().startsWith("http://", ignoreCase = true))
            connectionTokenStatus.text = when (state.tokenStatus) {
                TokenStatus.AVAILABLE -> getString(R.string.connection_token_present)
                TokenStatus.UNREADABLE -> getString(R.string.connection_token_unreadable)
                TokenStatus.ABSENT -> getString(R.string.connection_token_absent)
            }
            connectionTokenWarning.setVisible(state.tokenStatus == TokenStatus.UNREADABLE)

            connectionSave.isEnabled = !state.busy
            connectionTest.isEnabled = !state.busy
            connectionProgress.setVisible(state.busy)
            connectionError.setVisible(state.error != null)
            connectionError.text = state.error?.userMessage(requireContext())
            connectionStatusText.text = statusText(state)
        }
        rendering = false

        state.notice?.let { notice ->
            showMessage(
                when (notice) {
                    ConnectionNotice.SAVED -> R.string.connection_saved
                    ConnectionNotice.SAVED_AND_ONLINE -> R.string.connection_saved_and_online
                },
            )
            viewModel.consumeNotice()
        }
    }

    private fun statusText(state: ConnectionUiState): String {
        val status = state.status
        if (status == null) {
            return if (state.endpointText.isBlank()) {
                getString(R.string.connection_not_configured)
            } else {
                getString(R.string.connection_not_tested)
            }
        }
        val serverTime = Instant.ofEpochSecond(status.nowUnix)
            .atZone(ZoneId.systemDefault())
            .format(DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss", Locale.ROOT))
        val lines = mutableListOf(
            getString(R.string.connection_online),
            getString(R.string.connection_version, status.version, status.schemaVersion),
            getString(R.string.connection_server_time, serverTime),
            getString(R.string.connection_events, status.pendingEvents, status.leasedEvents),
            getString(R.string.connection_schedules, status.enabledSchedules, status.runningTimers),
        )
        if (!status.scenarioExecutionSupported) {
            lines += getString(R.string.connection_scenario)
        }
        return lines.joinToString("\n")
    }
}

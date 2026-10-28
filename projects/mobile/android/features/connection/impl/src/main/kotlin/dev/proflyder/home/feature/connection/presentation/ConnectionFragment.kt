package dev.proflyder.home.feature.connection.presentation

import android.os.Bundle
import android.view.View
import android.widget.TextView
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.Fragment
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.button.MaterialButton
import com.google.android.material.snackbar.Snackbar
import com.google.android.material.textfield.TextInputEditText
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.connection.impl.R
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlinx.coroutines.launch

class ConnectionFragment : Fragment(R.layout.fragment_connection) {
    private val viewModel: ConnectionViewModel by viewModels {
        viewModelFactory(ConnectionViewModel::class.java)
    }

    private lateinit var endpoint: TextInputEditText
    private lateinit var token: TextInputEditText
    private lateinit var connect: TextInputEditText
    private lateinit var read: TextInputEditText
    private lateinit var httpWarning: TextView
    private lateinit var tokenStatus: TextView
    private lateinit var tokenWarning: TextView
    private lateinit var error: TextView
    private lateinit var statusText: TextView
    private lateinit var save: MaterialButton
    private lateinit var test: MaterialButton
    private lateinit var progress: View
    private var rendering = false

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        endpoint = view.findViewById(R.id.connection_endpoint)
        token = view.findViewById(R.id.connection_token)
        connect = view.findViewById(R.id.connection_connect_timeout)
        read = view.findViewById(R.id.connection_read_timeout)
        httpWarning = view.findViewById(R.id.connection_http_warning)
        tokenStatus = view.findViewById(R.id.connection_token_status)
        tokenWarning = view.findViewById(R.id.connection_token_warning)
        error = view.findViewById(R.id.connection_error)
        statusText = view.findViewById(R.id.connection_status_text)
        save = view.findViewById(R.id.connection_save)
        test = view.findViewById(R.id.connection_test)
        progress = view.findViewById(R.id.connection_progress)

        endpoint.doAfterTextChanged { if (!rendering) viewModel.setEndpoint(it?.toString().orEmpty()) }
        token.doAfterTextChanged { if (!rendering) viewModel.setToken(it?.toString().orEmpty()) }
        connect.doAfterTextChanged { if (!rendering) viewModel.setConnectSeconds(it?.toString().orEmpty()) }
        read.doAfterTextChanged { if (!rendering) viewModel.setReadSeconds(it?.toString().orEmpty()) }
        save.setOnClickListener { viewModel.save() }
        test.setOnClickListener { viewModel.test() }
        view.findViewById<View>(R.id.connection_clear_token).setOnClickListener {
            token.setText("")
            viewModel.requestClearToken()
        }

        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.state.collect(::render)
            }
        }
    }

    private fun render(state: ConnectionUiState) {
        rendering = true
        if (endpoint.text?.toString() != state.endpointText) endpoint.setText(state.endpointText)
        if (token.text?.toString() != state.tokenInput) token.setText(state.tokenInput)
        if (connect.text?.toString() != state.connectSeconds) connect.setText(state.connectSeconds)
        if (read.text?.toString() != state.readSeconds) read.setText(state.readSeconds)

        httpWarning.visibility =
            if (state.endpointText.trim().startsWith("http://", ignoreCase = true)) View.VISIBLE else View.GONE
        tokenStatus.text = when (state.tokenStatus) {
            TokenStatus.AVAILABLE -> getString(R.string.connection_token_present)
            TokenStatus.UNREADABLE -> getString(R.string.connection_token_unreadable)
            TokenStatus.ABSENT -> getString(R.string.connection_token_absent)
        }
        tokenWarning.visibility =
            if (state.tokenStatus == TokenStatus.UNREADABLE) View.VISIBLE else View.GONE

        save.isEnabled = !state.busy
        test.isEnabled = !state.busy
        progress.visibility = if (state.busy) View.VISIBLE else View.GONE
        error.visibility = if (state.error != null) View.VISIBLE else View.GONE
        error.text = state.error?.userMessage(requireContext())
        statusText.text = statusText(state)
        rendering = false

        state.notice?.let { notice ->
            val message = when (notice) {
                ConnectionNotice.SAVED -> R.string.connection_saved
                ConnectionNotice.SAVED_AND_ONLINE -> R.string.connection_saved_and_online
            }
            Snackbar.make(requireView(), message, Snackbar.LENGTH_SHORT).show()
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

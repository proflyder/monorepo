package dev.proflyder.home.feature.connection.presentation

import android.os.Bundle
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.TextBinder
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.connection.impl.R
import dev.proflyder.home.feature.connection.impl.databinding.FragmentConnectionBinding
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

internal class ConnectionFragment :
    ScreenFragment<FragmentConnectionBinding, ConnectionUiState, ConnectionIntent, ConnectionEvent>(
        FragmentConnectionBinding.Companion::inflate,
    ) {
    override val viewModel: ConnectionViewModel by screenViewModel()

    private lateinit var tokenBinder: TextBinder

    override fun onScreenCreated(binding: FragmentConnectionBinding, savedInstanceState: Bundle?) {
        binding.connectionScroll.applyScrollBottomInset()
        binding.connectionScroll.keepFocusedVisible()
        tokenBinder = bindText(binding.connectionToken, ConnectionIntent::EditToken)
        binding.connectionRefresh.setupPullToRefresh { dispatch(ConnectionIntent.Test) }
        binding.connectionTest.setOnClickListener { dispatch(ConnectionIntent.Test) }
        binding.connectionRefreshConfig.setOnClickListener { dispatch(ConnectionIntent.RefreshConfig) }
        binding.connectionClearToken.setOnClickListener { dispatch(ConnectionIntent.ClearToken) }
    }

    override fun onEvent(event: ConnectionEvent) {
        when (event) {
            is ConnectionEvent.Notice -> showMessage(noticeText(event.notice))
        }
    }

    private fun noticeText(notice: ConnectionNotice): Int = when (notice) {
        ConnectionNotice.SAVED -> R.string.connection_saved
        ConnectionNotice.SAVED_AND_ONLINE -> R.string.connection_saved_and_online
        ConnectionNotice.CONFIG_REFRESHED -> R.string.connection_remote_refreshed
    }

    override fun render(state: ConnectionUiState) {
        val loaded = loadedConfiguration(state.remote)
        tokenBinder.render(state.tokenInput)
        with(binding) {
            connectionRemoteState.text = remoteStateText(state.remote)
            connectionRemoteScheduler.text = loaded?.endpoint?.baseUrl()
                ?: getString(R.string.connection_remote_not_provided)
            connectionRemoteControl.text = when {
                loaded == null -> getString(R.string.connection_remote_not_provided)
                else -> loaded.controlEndpoint?.baseUrl() ?: getString(R.string.connection_remote_not_set)
            }
            connectionRemoteIntercom.text = when {
                loaded == null -> getString(R.string.connection_remote_not_provided)
                else -> loaded.intercomEndpoint?.baseUrl() ?: getString(R.string.connection_remote_not_set_calls)
            }
            connectionRemoteTimeouts.text = loaded?.let {
                getString(
                    R.string.connection_remote_timeouts,
                    it.connectTimeoutMillis / 1000L,
                    it.readTimeoutMillis / 1000L,
                )
            } ?: getString(R.string.connection_remote_not_provided)

            connectionHttpWarning.setVisible(
                loaded?.endpoint?.baseUrl()?.startsWith("http://", ignoreCase = true) == true,
            )
            connectionTokenStatus.text = when (state.tokenStatus) {
                TokenStatus.AVAILABLE -> getString(R.string.connection_token_present)
                TokenStatus.UNREADABLE -> getString(R.string.connection_token_unreadable)
                TokenStatus.ABSENT -> getString(R.string.connection_token_absent)
            }
            connectionTokenWarning.setVisible(state.tokenStatus == TokenStatus.UNREADABLE)
            connectionClearToken.setVisible(state.tokenStatus != TokenStatus.ABSENT)

            connectionTest.isEnabled = !state.busy
            connectionRefreshConfig.isEnabled = !state.refreshingConfig
            connectionRefresh.isRefreshing = state.saving || state.testing
            connectionProgress.setVisible(state.busy)
            connectionError.setVisible(state.error != null)
            connectionError.text = state.error?.userMessage(requireContext())
            connectionStatusText.text = statusText(state, loaded != null)
        }
    }

    private fun remoteStateText(remote: RemoteConnectionState): String = when (remote) {
        RemoteConnectionState.Unavailable -> getString(R.string.connection_remote_unavailable)
        RemoteConnectionState.Loading -> getString(R.string.connection_remote_loading)
        RemoteConnectionState.Missing -> getString(R.string.connection_remote_missing)
        is RemoteConnectionState.Invalid -> when (remote.field) {
            null -> getString(R.string.connection_remote_invalid_json)
            Field.ENDPOINT -> getString(R.string.connection_remote_invalid_scheduler)
            Field.CONTROL_ENDPOINT -> getString(R.string.connection_remote_invalid_control)
            Field.INTERCOM_ENDPOINT -> getString(R.string.connection_remote_invalid_intercom)
            Field.TIMEOUT -> getString(R.string.connection_remote_invalid_timeout)
            else -> getString(R.string.connection_remote_invalid)
        }
        is RemoteConnectionState.FetchFailed ->
            if (remote.lastKnown != null) {
                getString(R.string.connection_remote_fetch_failed_cached)
            } else {
                getString(R.string.connection_remote_fetch_failed)
            }
        is RemoteConnectionState.Loaded -> getString(R.string.connection_remote_loaded)
    }

    private fun loadedConfiguration(remote: RemoteConnectionState): RemoteConnectionState.Loaded? = when (remote) {
        is RemoteConnectionState.Loaded -> remote
        is RemoteConnectionState.FetchFailed -> remote.lastKnown
        else -> null
    }

    private fun statusText(state: ConnectionUiState, configured: Boolean): String {
        val status = state.status
        if (status == null) {
            return if (configured) {
                getString(R.string.connection_not_tested)
            } else {
                getString(R.string.connection_not_configured)
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

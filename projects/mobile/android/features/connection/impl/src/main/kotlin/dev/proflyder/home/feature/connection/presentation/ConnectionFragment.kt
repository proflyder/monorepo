package dev.proflyder.home.feature.connection.presentation

import dev.proflyder.home.core.presentation.bindText
import android.os.Bundle
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.presentation.ScreenFragment
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

    override fun onViewReady(binding: FragmentConnectionBinding, savedInstanceState: Bundle?) {
        binding.connectionScroll.applyScrollBottomInset()
        binding.connectionScroll.keepFocusedVisible()
        val tokenBinder = bindText(binding.connectionToken, ConnectionIntent::EditToken)
        binding.connectionRefresh.setupPullToRefresh { dispatch(ConnectionIntent.Test) }
        binding.connectionTest.setOnClickListener { dispatch(ConnectionIntent.Test) }
        binding.connectionRefreshConfig.setOnClickListener { dispatch(ConnectionIntent.RefreshConfig) }
        binding.connectionClearToken.setOnClickListener { dispatch(ConnectionIntent.ClearToken) }
        render(ConnectionUiState::tokenInput, tokenBinder::render)
        render(ConnectionUiState::remote, ::renderRemote)
        render(ConnectionUiState::tokenStatus, ::renderTokenStatus)
        render(::connectionProgressOf, ::renderProgress)
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

    private fun renderRemote(remote: RemoteConnectionState) = with(binding) {
        val loaded = loadedConfiguration(remote)
        connectionRemoteState.text = remoteStateText(remote)
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
            getString(R.string.connection_remote_timeouts, it.connectTimeoutMillis / 1000L, it.readTimeoutMillis / 1000L)
        } ?: getString(R.string.connection_remote_not_provided)
        connectionHttpWarning.setVisible(loaded?.endpoint?.baseUrl()?.startsWith("http://", ignoreCase = true) == true)
    }

    private fun renderTokenStatus(status: TokenStatus) = with(binding) {
        connectionTokenStatus.text = when (status) {
            TokenStatus.AVAILABLE -> getString(R.string.connection_token_present)
            TokenStatus.UNREADABLE -> getString(R.string.connection_token_unreadable)
            TokenStatus.ABSENT -> getString(R.string.connection_token_absent)
            TokenStatus.LOADING -> getString(R.string.connection_token_loading)
        }
        connectionTokenWarning.setVisible(status == TokenStatus.UNREADABLE)
        connectionClearToken.setVisible(status in CLEARABLE_TOKEN)
    }

    private fun renderProgress(progress: ConnectionProgress) = with(binding) {
        connectionTest.isEnabled = !progress.busy
        connectionRefreshConfig.isEnabled = !progress.refreshingConfig
        connectionRefresh.isRefreshing = progress.checking
        connectionProgress.setVisible(progress.busy)
        connectionError.setVisible(progress.error != null)
        connectionError.text = progress.error?.userMessage(requireContext())
        connectionStatusText.text = statusText(progress.status, progress.configured)
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

    private fun statusText(status: SchedulerStatus?, configured: Boolean): String {
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

private val CLEARABLE_TOKEN = setOf(TokenStatus.AVAILABLE, TokenStatus.UNREADABLE)

internal data class ConnectionProgress(
    val busy: Boolean,
    val refreshingConfig: Boolean,
    val checking: Boolean,
    val error: RemoteFailure?,
    val status: SchedulerStatus?,
    val configured: Boolean,
)

internal fun connectionProgressOf(state: ConnectionUiState): ConnectionProgress = ConnectionProgress(
    busy = state.busy,
    refreshingConfig = state.refreshingConfig,
    checking = state.saving || state.testing,
    error = state.error,
    status = state.status,
    configured = state.remote is RemoteConnectionState.Loaded || (state.remote as? RemoteConnectionState.FetchFailed)?.lastKnown != null,
)

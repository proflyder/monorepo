package dev.proflyder.home.feature.connection.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.connection.ConnectionRepository
import dev.proflyder.home.feature.connection.TokenUpdate
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal enum class ConnectionNotice {
    SAVED,
    SAVED_AND_ONLINE,
    CONFIG_REFRESHED,
}

internal sealed interface ConnectionIntent {
    data class EditToken(val value: String) : ConnectionIntent
    data object ClearToken : ConnectionIntent
    data object Test : ConnectionIntent
    data object RefreshConfig : ConnectionIntent
}

internal sealed interface ConnectionEvent {
    data class Notice(val notice: ConnectionNotice) : ConnectionEvent
}

internal data class ConnectionUiState(
    val tokenInput: String = "",
    val clearTokenRequested: Boolean = false,
    val hasStoredToken: Boolean = false,
    val tokenStatus: TokenStatus = TokenStatus.ABSENT,
    val remote: RemoteConnectionState = RemoteConnectionState.Unavailable,
    val refreshingConfig: Boolean = false,
    val saving: Boolean = false,
    val testing: Boolean = false,
    val error: RemoteFailure? = null,
    val status: SchedulerStatus? = null,
) {
    val busy: Boolean get() = saving || testing || refreshingConfig
}

@Inject
internal class ConnectionViewModel(
    private val repository: ConnectionRepository,
) : StateViewModel<ConnectionUiState, ConnectionIntent, ConnectionEvent>(initialState(repository)) {

    init {
        launch {
            repository.remoteConfig.collect { remote ->
                updateState { copy(remote = remote) }
            }
        }
        launch {
            repository.tokenStatus.collect { tokenStatus ->
                updateState { copy(tokenStatus = tokenStatus) }
            }
        }
        launch {
            repository.config.collect { config ->
                updateState { copy(hasStoredToken = config.hasToken) }
            }
        }
    }

    override fun handle(intent: ConnectionIntent) {
        when (intent) {
            is ConnectionIntent.EditToken -> setToken(intent.value)
            ConnectionIntent.ClearToken -> requestClearToken()
            ConnectionIntent.Test -> persist(testAfter = true)
            ConnectionIntent.RefreshConfig -> refreshConfig()
        }
    }

    private fun setToken(value: String) {
        updateState { copy(tokenInput = value, clearTokenRequested = false, error = null, status = null) }
    }

    private fun requestClearToken() {
        updateState { copy(tokenInput = "", clearTokenRequested = true) }
    }

    private fun refreshConfig() {
        if (currentState.refreshingConfig) return
        updateState { copy(refreshingConfig = true, error = null) }
        launch {
            val refreshed = repository.refreshRemoteConfig()
            updateState { copy(refreshingConfig = false, error = refreshed.failureOrNull()) }
            if (refreshed is Outcome.Success) emit(ConnectionEvent.Notice(ConnectionNotice.CONFIG_REFRESHED))
        }
    }

    private fun persist(testAfter: Boolean) {
        val current = currentState
        if (current.saving || current.testing) return
        updateState { copy(saving = true, testing = testAfter, error = null, status = null) }
        launch { saveThenMaybeTest(current.tokenUpdate(), testAfter) }
    }

    private suspend fun saveThenMaybeTest(update: TokenUpdate, testAfter: Boolean) {
        when (val saved = repository.saveToken(update)) {
            is Outcome.Failure -> updateState { copy(saving = false, testing = false, error = saved.failure) }
            is Outcome.Success -> {
                applySaved(saved.value)
                if (testAfter) applyTestResult(repository.testConnection())
                if (!testAfter) emit(ConnectionEvent.Notice(ConnectionNotice.SAVED))
            }
        }
    }

    private fun applySaved(config: ConnectionConfig) {
        updateState {
            copy(
                tokenInput = "",
                clearTokenRequested = false,
                hasStoredToken = config.hasToken,
                tokenStatus = repository.tokenStatus.value,
                saving = false,
            )
        }
    }

    private suspend fun applyTestResult(result: Outcome<SchedulerStatus>) {
        when (result) {
            is Outcome.Success -> {
                updateState { copy(testing = false, status = result.value) }
                emit(ConnectionEvent.Notice(ConnectionNotice.SAVED_AND_ONLINE))
            }
            is Outcome.Failure -> updateState { copy(testing = false, error = result.failure) }
        }
    }
}

internal fun ConnectionUiState.tokenUpdate(): TokenUpdate = when {
    clearTokenRequested -> TokenUpdate.Clear
    tokenInput.isNotBlank() -> TokenUpdate.Replace(tokenInput)
    else -> TokenUpdate.Keep
}

internal fun initialState(repository: ConnectionRepository): ConnectionUiState {
    val config = repository.config.value
    return ConnectionUiState(
        hasStoredToken = config.hasToken,
        tokenStatus = repository.tokenStatus.value,
        remote = repository.remoteConfig.value,
    )
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(ConnectionViewModel::class)
@Inject
class ConnectionViewModelSource internal constructor(
    private val viewModel: () -> ConnectionViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

package dev.proflyder.home.feature.connection.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.connection.ConnectionRepository
import dev.proflyder.home.feature.connection.TokenUpdate
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

enum class ConnectionNotice {
    SAVED,
    SAVED_AND_ONLINE,
    CONFIG_REFRESHED,
}

data class ConnectionUiState(
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
    val notice: ConnectionNotice? = null,
) {
    val busy: Boolean get() = saving || testing || refreshingConfig
}

@Inject
internal class ConnectionViewModel(
    private val repository: ConnectionRepository,
) : BaseViewModel<ConnectionUiState>(initialState(repository)) {

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
    }

    fun setToken(value: String) {
        updateState { copy(tokenInput = value, clearTokenRequested = false, error = null, status = null) }
    }

    fun requestClearToken() {
        updateState { copy(tokenInput = "", clearTokenRequested = true) }
    }

    fun test() = persist(testAfter = true)

    fun refreshConfig() {
        val current = currentState
        if (current.refreshingConfig) return
        updateState { copy(refreshingConfig = true, error = null, notice = null) }
        launch {
            suspendRunCatching {
                repository.refreshRemoteConfig()
            }.onSuccess {
                updateState { copy(refreshingConfig = false, notice = ConnectionNotice.CONFIG_REFRESHED) }
            }.onFailure { failure ->
                if (failure !is Exception) throw failure
                updateState {
                    copy(
                        refreshingConfig = false,
                        error = RemoteFailure.Offline("Remote Config could not be refreshed."),
                    )
                }
            }
        }
    }

    fun consumeNotice() {
        updateState { copy(notice = null) }
    }

    private fun persist(testAfter: Boolean) {
        val current = currentState
        if (current.saving || current.testing) return

        val tokenUpdate = when {
            current.clearTokenRequested -> TokenUpdate.Clear
            current.tokenInput.isNotBlank() -> TokenUpdate.Replace(current.tokenInput)
            else -> TokenUpdate.Keep
        }

        updateState {
            copy(
                saving = true,
                testing = testAfter,
                error = null,
                notice = null,
                status = null,
            )
        }

        launch {
            when (val saved = repository.saveToken(tokenUpdate)) {
                is Outcome.Failure -> updateState {
                    copy(saving = false, testing = false, error = saved.failure)
                }
                is Outcome.Success -> {
                    val config: ConnectionConfig = saved.value
                    updateState {
                        copy(
                            tokenInput = "",
                            clearTokenRequested = false,
                            hasStoredToken = config.hasToken,
                            tokenStatus = repository.tokenStatus.value,
                            saving = false,
                        )
                    }
                    if (!testAfter) {
                        updateState { copy(notice = ConnectionNotice.SAVED) }
                        return@launch
                    }
                    when (val status = repository.testConnection()) {
                        is Outcome.Success -> updateState {
                            copy(
                                testing = false,
                                status = status.value,
                                notice = ConnectionNotice.SAVED_AND_ONLINE,
                            )
                        }
                        is Outcome.Failure -> updateState {
                            copy(testing = false, error = status.failure)
                        }
                    }
                }
            }
        }
    }
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

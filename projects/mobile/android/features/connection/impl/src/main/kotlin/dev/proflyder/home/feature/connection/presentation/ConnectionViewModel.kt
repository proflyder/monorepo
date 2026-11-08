package dev.proflyder.home.feature.connection.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.connection.ConnectionRepository
import dev.proflyder.home.feature.connection.TokenUpdate
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

enum class ConnectionNotice {
    SAVED,
    SAVED_AND_ONLINE,
    CONFIG_REFRESHED,
    INTERCOM_TOKEN_SAVED,
}

data class ConnectionUiState(
    val tokenInput: String = "",
    val clearTokenRequested: Boolean = false,
    val hasStoredToken: Boolean = false,
    val tokenStatus: TokenStatus = TokenStatus.ABSENT,
    val intercomTokenInput: String = "",
    val clearIntercomTokenRequested: Boolean = false,
    val hasStoredIntercomToken: Boolean = false,
    val intercomTokenStatus: TokenStatus = TokenStatus.ABSENT,
    val savingIntercom: Boolean = false,
    val remote: RemoteConnectionState = RemoteConnectionState.Unavailable,
    val refreshingConfig: Boolean = false,
    val saving: Boolean = false,
    val testing: Boolean = false,
    val error: RemoteFailure? = null,
    val status: SchedulerStatus? = null,
    val notice: ConnectionNotice? = null,
) {
    val busy: Boolean get() = saving || testing || refreshingConfig || savingIntercom
}

class ConnectionViewModel(
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
        launch {
            repository.intercomTokenStatus.collect { status ->
                updateState { copy(intercomTokenStatus = status) }
            }
        }
    }

    fun setToken(value: String) {
        updateState { copy(tokenInput = value, clearTokenRequested = false, error = null, status = null) }
    }

    fun requestClearToken() {
        updateState { copy(tokenInput = "", clearTokenRequested = true) }
    }

    fun setIntercomToken(value: String) {
        updateState {
            copy(intercomTokenInput = value, clearIntercomTokenRequested = false, error = null)
        }
    }

    fun requestClearIntercomToken() {
        updateState { copy(intercomTokenInput = "", clearIntercomTokenRequested = true) }
    }

    /** Saves only the intercom phone token; endpoints stay Remote-Config owned. */
    fun saveIntercomToken() {
        val current = currentState
        if (current.savingIntercom) return
        val update = when {
            current.clearIntercomTokenRequested -> TokenUpdate.Clear
            current.intercomTokenInput.isNotBlank() -> TokenUpdate.Replace(current.intercomTokenInput)
            else -> return
        }
        updateState { copy(savingIntercom = true, error = null, notice = null) }
        launch {
            when (val saved = repository.saveIntercomToken(update)) {
                is Outcome.Failure -> updateState { copy(savingIntercom = false, error = saved.failure) }
                is Outcome.Success -> updateState {
                    copy(
                        savingIntercom = false,
                        intercomTokenInput = "",
                        clearIntercomTokenRequested = false,
                        hasStoredIntercomToken = saved.value.hasIntercomToken,
                        intercomTokenStatus = repository.intercomTokenStatus.value,
                        notice = ConnectionNotice.INTERCOM_TOKEN_SAVED,
                    )
                }
            }
        }
    }

    fun test() = persist(testAfter = true)

    fun refreshConfig() {
        val current = currentState
        if (current.refreshingConfig) return
        updateState { copy(refreshingConfig = true, error = null, notice = null) }
        launch {
            try {
                repository.refreshRemoteConfig()
                updateState { copy(refreshingConfig = false, notice = ConnectionNotice.CONFIG_REFRESHED) }
            } catch (_: Exception) {
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

@ContributesIntoSet(AppScope::class)
@Inject
class ConnectionViewModelFactoryContributor(
    private val repository: ConnectionRepository,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ConnectionViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        ConnectionViewModel(repository)
    }
}

internal fun initialState(repository: ConnectionRepository): ConnectionUiState {
    val config = repository.config.value
    return ConnectionUiState(
        hasStoredToken = config.hasToken,
        tokenStatus = repository.tokenStatus.value,
        hasStoredIntercomToken = config.hasIntercomToken,
        intercomTokenStatus = repository.intercomTokenStatus.value,
        remote = repository.remoteConfig.value,
    )
}

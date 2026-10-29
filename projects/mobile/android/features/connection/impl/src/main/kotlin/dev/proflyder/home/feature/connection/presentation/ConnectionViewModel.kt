package dev.proflyder.home.feature.connection.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.connection.ConnectionDraft
import dev.proflyder.home.feature.connection.ConnectionRepository
import dev.proflyder.home.feature.connection.TokenUpdate
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.launch

enum class ConnectionNotice {
    SAVED,
    SAVED_AND_ONLINE,
}

data class ConnectionUiState(
    val endpointText: String = "",
    val tokenInput: String = "",
    val clearTokenRequested: Boolean = false,
    val hasStoredToken: Boolean = false,
    val tokenStatus: TokenStatus = TokenStatus.ABSENT,
    val connectSeconds: String = "5",
    val readSeconds: String = "20",
    val saving: Boolean = false,
    val testing: Boolean = false,
    val error: RemoteFailure? = null,
    val status: SchedulerStatus? = null,
    val notice: ConnectionNotice? = null,
) {
    val busy: Boolean get() = saving || testing
}

class ConnectionViewModel(
    private val repository: ConnectionRepository,
) : BaseViewModel<ConnectionUiState>(initialState(repository)) {

    fun setEndpoint(value: String) {
        mutableState.value = mutableState.value.copy(endpointText = value, error = null, status = null)
    }

    fun setToken(value: String) {
        mutableState.value = mutableState.value.copy(tokenInput = value, clearTokenRequested = false, error = null, status = null)
    }

    fun requestClearToken() {
        mutableState.value = mutableState.value.copy(tokenInput = "", clearTokenRequested = true)
    }

    fun setConnectSeconds(value: String) {
        mutableState.value = mutableState.value.copy(connectSeconds = value, error = null, status = null)
    }

    fun setReadSeconds(value: String) {
        mutableState.value = mutableState.value.copy(readSeconds = value, error = null, status = null)
    }

    fun save() = persist(testAfter = false)

    fun test() = persist(testAfter = true)

    fun consumeNotice() {
        mutableState.value = mutableState.value.copy(notice = null)
    }

    private fun persist(testAfter: Boolean) {
        val current = mutableState.value
        if (current.busy) return

        val connect = current.connectSeconds.toIntOrNull()
        val read = current.readSeconds.toIntOrNull()
        if (connect == null || read == null || connect !in 1..120 || read !in 1..120) {
            mutableState.value = current.copy(
                error = RemoteFailure.Validation(dev.proflyder.home.core.model.Field.TIMEOUT),
            )
            return
        }

        val tokenUpdate = when {
            current.clearTokenRequested -> TokenUpdate.Clear
            current.tokenInput.isNotBlank() -> TokenUpdate.Replace(current.tokenInput)
            else -> TokenUpdate.Keep
        }

        mutableState.value = current.copy(
            saving = true,
            testing = testAfter,
            error = null,
            notice = null,
            status = null,
        )

        viewModelScope.launch {
            when (val saved = repository.save(ConnectionDraft(current.endpointText, tokenUpdate, connect, read))) {
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    saving = false,
                    testing = false,
                    error = saved.failure,
                )
                is Outcome.Success -> {
                    val config: ConnectionConfig = saved.value
                    mutableState.value = mutableState.value.copy(
                        endpointText = config.endpoint?.baseUrl().orEmpty(),
                        tokenInput = "",
                        clearTokenRequested = false,
                        hasStoredToken = config.hasToken,
                        tokenStatus = repository.tokenStatus.value,
                        saving = false,
                    )
                    if (!testAfter) {
                        mutableState.value = mutableState.value.copy(notice = ConnectionNotice.SAVED)
                        return@launch
                    }
                    when (val status = repository.testConnection()) {
                        is Outcome.Success -> mutableState.value = mutableState.value.copy(
                            testing = false,
                            status = status.value,
                            notice = ConnectionNotice.SAVED_AND_ONLINE,
                        )
                        is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                            testing = false,
                            error = status.failure,
                        )
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

private fun initialState(repository: ConnectionRepository): ConnectionUiState {
    val config = repository.config.value
    return ConnectionUiState(
        endpointText = config.endpoint?.baseUrl().orEmpty(),
        hasStoredToken = config.hasToken,
        tokenStatus = repository.tokenStatus.value,
        connectSeconds = (config.connectTimeoutMillis / 1000L).toString(),
        readSeconds = (config.readTimeoutMillis / 1000L).toString(),
    )
}

package dev.proflyder.home.feature.access.data

import dev.proflyder.home.core.access.AccessRefresher
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.AccessApi
import dev.proflyder.home.feature.access.AccessRepository
import dev.proflyder.home.feature.access.AccessTokensState
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class RemoteAccessRepository(
    private val api: AccessApi,
    private val access: AccessRefresher,
    private val scope: CoroutineScope,
) : AccessRepository {
    private val mutableState = MutableStateFlow(AccessTokensState())
    private var loading: Job? = null

    override val state: StateFlow<AccessTokensState> = mutableState.asStateFlow()

    init {
        refresh()
    }

    override fun refresh() {
        access.refresh()
        mutableState.update { it.copy(loading = true, failure = null) }
        loading?.cancel()
        loading = scope.launch { load() }
    }

    override suspend fun create(name: String, modules: Set<AppModule>): Outcome<CreatedAccessToken> =
        api.createToken(name, modules).also { result ->
            if (result is Outcome.Success) mutableState.update { it.copy(tokens = it.tokens + result.value.access) }
            access.refresh()
        }

    override suspend fun update(id: String, name: String, modules: Set<AppModule>): Outcome<AccessToken> =
        api.updateToken(id, name, modules).also { result ->
            if (result is Outcome.Success) replace(result.value)
            access.refresh()
        }

    override suspend fun revoke(id: String): Outcome<Unit> = api.revokeToken(id).also { result ->
        if (result is Outcome.Success) {
            mutableState.update { it.copy(tokens = it.tokens.filterNot { token -> token.id == id }) }
        }
        access.refresh()
    }

    private suspend fun load() {
        when (val result = api.tokens()) {
            is Outcome.Success -> mutableState.value = AccessTokensState(loading = false, tokens = result.value)
            is Outcome.Failure -> mutableState.update { it.copy(loading = false, failure = result.failure) }
        }
    }

    private fun replace(token: AccessToken) {
        mutableState.update { state ->
            state.copy(tokens = state.tokens.map { if (it.id == token.id) token else it })
        }
    }
}

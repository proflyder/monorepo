package dev.proflyder.home.feature.access

import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.flow.StateFlow

data class AccessTokensState(
    val loading: Boolean = true,
    val tokens: List<AccessToken> = emptyList(),
    val failure: RemoteFailure? = null,
)

interface AccessRepository {
    val state: StateFlow<AccessTokensState>

    fun refresh()

    suspend fun create(name: String, modules: Set<AppModule>): Outcome<CreatedAccessToken>

    suspend fun update(id: String, name: String, modules: Set<AppModule>): Outcome<AccessToken>

    suspend fun revoke(id: String): Outcome<Unit>
}

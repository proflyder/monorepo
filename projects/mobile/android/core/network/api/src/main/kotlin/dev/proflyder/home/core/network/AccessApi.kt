package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome

interface AccessApi {
    suspend fun me(): Outcome<AppAccess>

    suspend fun tokens(): Outcome<List<AccessToken>>

    suspend fun createToken(name: String, modules: Set<AppModule>): Outcome<CreatedAccessToken>

    suspend fun updateToken(id: String, name: String?, modules: Set<AppModule>?): Outcome<AccessToken>

    suspend fun revokeToken(id: String): Outcome<Unit>
}

fun interface ModuleForbiddenSink {
    fun moduleForbidden(module: String)
}

package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import okhttp3.Authenticator
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route

internal class TokenRefreshAuthenticator(
    private val settings: ConnectionSettingsStore,
) : Authenticator {
    override fun authenticate(route: Route?, response: Response): Request? {
        val token = settings.config.value.bearerToken?.trim()
        return when {
            !ConnectionConfig.isTokenValid(token) -> null
            response.request.header("Authorization") == "Bearer $token" -> null
            response.priorResponse != null -> null
            else -> response.request.newBuilder().header("Authorization", "Bearer $token").build()
        }
    }
}

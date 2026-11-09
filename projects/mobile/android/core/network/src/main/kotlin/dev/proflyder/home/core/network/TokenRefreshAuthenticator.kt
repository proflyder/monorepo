package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import okhttp3.Authenticator
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route

/**
 * Refreshes the bearer token once after a 401 pushed from a rotated token.
 *
 * ConnectionInterceptor is an application interceptor and does not rerun for an
 * authenticator follow-up, so this class sets the header itself from the current
 * settings. Returning null when the token did not change (or when a prior
 * attempt already ran) prevents an authentication loop; the error adapter then
 * surfaces the existing `Unauthorized` failure. The token still travels only in
 * the Authorization header and is never logged.
 */
class TokenRefreshAuthenticator(
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

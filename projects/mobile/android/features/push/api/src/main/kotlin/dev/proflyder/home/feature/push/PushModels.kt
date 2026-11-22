package dev.proflyder.home.feature.push

import kotlinx.coroutines.flow.StateFlow

object PushIntents {
    const val EXTRA_DEEP_LINK: String = "dev.proflyder.home.extra.DEEP_LINK"

    const val EXTRA_FIND: String = "dev.proflyder.home.extra.FIND"
}

data class PushPermissionStatus(
    val notifications: Boolean = false,
    val dndAccess: Boolean = false,
    val fullScreenIntent: Boolean = false,
) {
    val canRing: Boolean get() = notifications && dndAccess && fullScreenIntent
}

data class PushRegistration(
    val token: String? = null,
    val registeredAtMillis: Long = 0,
    val deviceName: String = "",
    val registeredName: String = "",
    val appVersion: String = "",
    val server: String = "",
) {
    fun tokenHint(): String = token?.takeLast(TOKEN_HINT_CHARS).orEmpty()

    private companion object {
        const val TOKEN_HINT_CHARS = 6
    }
}

data class PushSyncStatus(
    val busy: Boolean = false,
    val failed: Boolean = false,
)

data class PushRegistrationRecord(
    val token: String,
    val registeredAtMillis: Long,
    val deviceName: String,
    val appVersion: String,
    val server: String,
)

interface PushRegistrationStore {
    val registration: StateFlow<PushRegistration>

    suspend fun load()

    suspend fun saved(record: PushRegistrationRecord)

    suspend fun savedName(name: String)
}

interface PushTokenSource {
    val token: StateFlow<String?>

    fun refreshed(token: String)

    fun revoked(token: String)
}

interface PushRegistrar {
    val status: StateFlow<PushSyncStatus>

    suspend fun registerNow(): Boolean

    suspend fun saveDeviceName(name: String): Boolean
}

interface PushPermissionProbe {
    fun status(): PushPermissionStatus
}

data class PushDeepLink(val route: String) {
    companion object {
        const val SCHEME = "home"

        fun parse(raw: String?): PushDeepLink? {
            val value = raw?.trim().orEmpty()
            if (!value.startsWith("$SCHEME://")) return null
            val route = value.removePrefix("$SCHEME://").trim('/')
            if (route.isBlank() || route.length > MAX_ROUTE_CHARS) return null
            if (!route.all { it.isLetterOrDigit() || it in "/-_.?=&" }) return null
            return PushDeepLink(route)
        }

        internal const val MAX_ROUTE_CHARS = 256
    }
}

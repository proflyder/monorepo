package dev.proflyder.home.feature.push

import kotlinx.coroutines.flow.StateFlow

/** Intent extras shared by the notification tap target and the app shell. */
object PushIntents {
    /** `home://…` route carried by a notification; the shell selects the tab. */
    const val EXTRA_DEEP_LINK: String = "dev.proflyder.home.extra.DEEP_LINK"

    /** Set when the ringing screen was opened, so Back returns to the app. */
    const val EXTRA_FIND: String = "dev.proflyder.home.extra.FIND"
}

/**
 * The three Android permissions a find-phone alarm depends on. Each one is
 * user-granted outside the normal runtime dialog on modern releases, so the
 * screen shows the real state instead of assuming it.
 */
data class PushPermissionStatus(
    /** `POST_NOTIFICATIONS` (runtime, Android 13+). Without it nothing is shown. */
    val notifications: Boolean = false,
    /** Notification-policy access, needed for the bypass-Do-Not-Disturb channel. */
    val dndAccess: Boolean = false,
    /** Full-screen intent allowance (Android 14+); the ringing screen needs it. */
    val fullScreenIntent: Boolean = false,
) {
    /** Whether the loud path can actually start from a killed app. */
    val canRing: Boolean get() = notifications && dndAccess && fullScreenIntent
}

/** Registration state as the settings screen shows it. */
data class PushRegistration(
    /** Last token the server accepted, `null` before the first success. */
    val token: String? = null,
    /** When the server last accepted it (unix millis); 0 = never. */
    val registeredAtMillis: Long = 0,
    /** True while a registration request is in flight. */
    val busy: Boolean = false,
    /** Last failure, already localized by the screen. */
    val failed: Boolean = false,
) {
    /** Registers the token digest for display: never show the whole secret. */
    fun tokenHint(): String = token?.takeLast(TOKEN_HINT_CHARS).orEmpty()

    private companion object {
        const val TOKEN_HINT_CHARS = 6
    }
}

/** Persists the last accepted registration so the screen can show it at launch. */
interface PushRegistrationStore {
    val registration: StateFlow<PushRegistration>

    /** Loads the persisted registration once, before the first render. */
    suspend fun load()

    suspend fun saved(token: String, registeredAtMillis: Long)

    suspend fun busy(busy: Boolean)

    suspend fun failed(failed: Boolean)
}

/** Registers and refreshes this device's FCM token. */
interface PushRegistrar {
    /** Fetches the current token and registers it with the home server. */
    suspend fun registerNow()

    /** Triggers the in-app flow that shows the missing permissions. */
    fun permissions(): PushPermissionStatus
}

/**
 * Deep link carried by a notification. Only the app's own scheme is accepted,
 * so a server-supplied string can never open another application.
 */
data class PushDeepLink(val route: String) {
    companion object {
        /** Scheme the app owns for in-app navigation. */
        const val SCHEME = "home"

        /** Parses `home://alarms` into `alarms`; anything else is ignored. */
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

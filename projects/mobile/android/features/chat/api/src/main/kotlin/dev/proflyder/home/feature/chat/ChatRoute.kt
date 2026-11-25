package dev.proflyder.home.feature.chat

import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

data class ChatLink(val sessionId: String?, val messageId: String?)

object ChatRoute {
    const val FEATURE_ID: String = "chat"
    const val ROOT: String = FEATURE_ID
    const val ARCHIVE: String = "chat/archive"
    const val SESSION: String = "session"
    const val MESSAGE: String = "message"
    const val LEGACY_SESSION_ID: String = "home-app"

    fun isChat(route: String): Boolean = route == ROOT || route.startsWith("$ROOT/")

    fun session(sessionId: String): String = "$ROOT/$SESSION/$sessionId"

    fun parse(route: String): ChatLink? {
        val parts = route.trim('/').split('/')
        if (parts.firstOrNull() != ROOT) return null
        return when {
            parts.size == 3 && parts[1] == MESSAGE -> ChatLink(LEGACY_SESSION_ID, parts[2].ifBlank { null })
            parts.size == 3 && parts[1] == SESSION -> ChatLink(parts[2].ifBlank { null }, null)
            parts.size == 5 && parts[1] == SESSION && parts[3] == MESSAGE -> ChatLink(parts[2].ifBlank { null }, parts[4].ifBlank { null })
            else -> null
        }
    }
}

object ChatFocus {
    private val pending = AtomicReference<String?>(null)
    private val visibleSession = AtomicReference<String?>(null)
    private val listVisible = AtomicBoolean(false)

    val isVisible: Boolean get() = listVisible.get() || visibleSession.get() != null

    fun isShowing(sessionId: String?): Boolean = listVisible.get() || sessionId != null && visibleSession.get() == sessionId

    fun request(messageId: String) {
        pending.set(messageId)
    }

    fun take(): String? = pending.getAndSet(null)

    fun peek(): String? = pending.get()

    fun setListVisible(value: Boolean) {
        listVisible.set(value)
    }

    fun setVisibleSession(sessionId: String?) {
        visibleSession.set(sessionId)
    }
}

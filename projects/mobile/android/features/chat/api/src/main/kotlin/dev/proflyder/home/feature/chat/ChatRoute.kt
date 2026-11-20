package dev.proflyder.home.feature.chat

import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

object ChatRoute {
    const val FEATURE_ID: String = "chat"
    const val ROOT: String = FEATURE_ID
    const val MESSAGE_PREFIX: String = "chat/message/"

    fun isChat(route: String): Boolean = route == ROOT || route.startsWith("$ROOT/")

    fun messageId(route: String): String? =
        route.takeIf { it.startsWith(MESSAGE_PREFIX) }?.removePrefix(MESSAGE_PREFIX)?.takeIf { it.isNotBlank() }
}

object ChatFocus {
    private val pending = AtomicReference<String?>(null)
    private val visible = AtomicBoolean(false)

    val isVisible: Boolean get() = visible.get()

    fun request(messageId: String) {
        pending.set(messageId)
    }

    fun take(): String? = pending.getAndSet(null)

    fun peek(): String? = pending.get()

    fun setVisible(value: Boolean) {
        visible.set(value)
    }
}

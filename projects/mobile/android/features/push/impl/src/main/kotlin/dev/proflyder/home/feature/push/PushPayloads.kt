package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncOp
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.feature.chat.ChatFocus
import dev.proflyder.home.feature.chat.ChatRoute

internal sealed interface PushMessage {
    sealed interface Shown : PushMessage

    data class Find(val reason: String) : Shown

    data class Sync(val signal: SyncSignal.Changed) : PushMessage

    data class Notify(
        val title: String,
        val body: String,
        val deepLink: PushDeepLink?,
        val module: String?,
    ) : Shown
}

internal object PushPayloads {
    const val KEY_TYPE: String = "type"
    const val KEY_REASON: String = "reason"
    const val KEY_TITLE: String = "title"
    const val KEY_BODY: String = "body"
    const val KEY_DEEP_LINK: String = "deepLink"
    const val KEY_MODULE: String = "module"
    const val MAX_MODULE_CHARS: Int = 64
    const val TYPE_FIND: String = "phone_find"
    const val TYPE_NOTIFY: String = "phone_notify"
    const val TYPE_SYNC: String = "sync"
    const val KEY_ENTITY: String = "entity"
    const val KEY_OP: String = "op"
    const val KEY_ID: String = "id"
    const val MAX_ID_CHARS: Int = 128

    const val MAX_REASON_CHARS: Int = 200
    const val MAX_TITLE_CHARS: Int = 120
    const val MAX_BODY_CHARS: Int = 500

    fun decode(data: Map<String, String>, notificationTitle: String?, notificationBody: String?): PushMessage? {
        val type = data[KEY_TYPE]?.trim()
        if (type == TYPE_FIND) return find(data)
        if (type == TYPE_SYNC) return sync(data)
        val title = firstNonBlank(data[KEY_TITLE], notificationTitle).take(MAX_TITLE_CHARS)
        val body = firstNonBlank(data[KEY_BODY], notificationBody).take(MAX_BODY_CHARS)
        if (title.isEmpty() || body.isEmpty()) return null
        return PushMessage.Notify(
            title = title,
            body = body,
            deepLink = PushDeepLink.parse(data[KEY_DEEP_LINK]),
            module = data[KEY_MODULE]?.trim()?.takeIf { it.isNotEmpty() }?.take(MAX_MODULE_CHARS),
        )
    }

    private fun find(data: Map<String, String>): PushMessage =
        PushMessage.Find(data[KEY_REASON].orEmpty().trim().take(MAX_REASON_CHARS))

    private fun sync(data: Map<String, String>): PushMessage? {
        val entity = SyncEntity.parse(data[KEY_ENTITY]) ?: return null
        val op = SyncOp.parse(data[KEY_OP]) ?: return null
        val id = data[KEY_ID]?.trim()?.takeIf { it.isNotEmpty() && it.length <= MAX_ID_CHARS }
        return PushMessage.Sync(SyncSignal.Changed(entity, op, id))
    }

    private fun firstNonBlank(first: String?, second: String?): String =
        first?.trim()?.takeIf { it.isNotEmpty() } ?: second?.trim().orEmpty()
}

internal object PushChatGate {
    fun shouldSkip(message: PushMessage.Notify, showing: (String?) -> Boolean = ChatFocus::isShowing): Boolean {
        val link = message.deepLink?.route?.let(ChatRoute::parse) ?: return false
        return showing(link.sessionId)
    }
}

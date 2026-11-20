package dev.proflyder.home.feature.push

import dev.proflyder.home.feature.chat.ChatFocus
import dev.proflyder.home.feature.chat.ChatRoute

sealed interface PushMessage {
    data class Find(val reason: String) : PushMessage

    data class Notify(
        val title: String,
        val body: String,
        val deepLink: PushDeepLink?,
    ) : PushMessage
}

object PushPayloads {
    const val KEY_TYPE: String = "type"
    const val KEY_REASON: String = "reason"
    const val KEY_TITLE: String = "title"
    const val KEY_BODY: String = "body"
    const val KEY_DEEP_LINK: String = "deepLink"
    const val TYPE_FIND: String = "phone_find"
    const val TYPE_NOTIFY: String = "phone_notify"

    const val MAX_REASON_CHARS: Int = 200
    const val MAX_TITLE_CHARS: Int = 120
    const val MAX_BODY_CHARS: Int = 500

    fun decode(data: Map<String, String>, notificationTitle: String?, notificationBody: String?): PushMessage? {
        val type = data[KEY_TYPE]?.trim()
        if (type == TYPE_FIND) return find(data)
        val title = firstNonBlank(data[KEY_TITLE], notificationTitle).take(MAX_TITLE_CHARS)
        val body = firstNonBlank(data[KEY_BODY], notificationBody).take(MAX_BODY_CHARS)
        if (title.isEmpty() || body.isEmpty()) return null
        return PushMessage.Notify(
            title = title,
            body = body,
            deepLink = PushDeepLink.parse(data[KEY_DEEP_LINK]),
        )
    }

    private fun find(data: Map<String, String>): PushMessage =
        PushMessage.Find(data[KEY_REASON].orEmpty().trim().take(MAX_REASON_CHARS))

    private fun firstNonBlank(first: String?, second: String?): String =
        first?.trim()?.takeIf { it.isNotEmpty() } ?: second?.trim().orEmpty()
}

object PushChatGate {
    fun shouldSkip(message: PushMessage.Notify, chatVisible: Boolean = ChatFocus.isVisible): Boolean =
        chatVisible && message.deepLink?.route?.let(ChatRoute::isChat) == true
}

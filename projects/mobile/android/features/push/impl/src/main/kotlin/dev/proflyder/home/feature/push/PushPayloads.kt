package dev.proflyder.home.feature.push

/**
 * One decoded FCM message. The server sends two shapes:
 *
 * * `data.type = phone_find` — a data-only high-priority message; the app
 *   starts the loud find-phone service and does not draw a tray notification
 *   itself;
 * * `phone_notify` / a `notification` payload — a normal notification with an
 *   optional in-app deep link.
 *
 * The parser never trusts the payload: unknown types, blank titles and foreign
 * deep links are rejected instead of rendering arbitrary content.
 */
sealed interface PushMessage {
    data class Find(val reason: String) : PushMessage

    data class Notify(
        val title: String,
        val body: String,
        val deepLink: PushDeepLink?,
    ) : PushMessage
}

/** Pure payload decoder, unit-tested without Android. */
object PushPayloads {
    const val KEY_TYPE: String = "type"
    const val KEY_REASON: String = "reason"
    const val KEY_TITLE: String = "title"
    const val KEY_BODY: String = "body"
    const val KEY_DEEP_LINK: String = "deepLink"
    const val TYPE_FIND: String = "phone_find"
    const val TYPE_NOTIFY: String = "phone_notify"

    /** Bound for the reason line the ringing screen shows. */
    const val MAX_REASON_CHARS: Int = 200
    /** Bound for a notification title before it is truncated for display. */
    const val MAX_TITLE_CHARS: Int = 120
    /** Bound for a notification body before it is truncated for display. */
    const val MAX_BODY_CHARS: Int = 500

    /**
     * Decodes the data map and the optional `notification` title/body. A
     * `phone_find` data message wins over a notification block, because the
     * server deliberately sends no notification block for it.
     */
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

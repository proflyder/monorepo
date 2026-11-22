package dev.proflyder.home.feature.chat.presentation

import dev.proflyder.home.core.designsystem.R as DesignR
import android.content.Context
import android.text.format.DateFormat
import android.text.format.DateUtils
import androidx.annotation.StringRes
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.feature.chat.impl.R
import java.time.LocalDate
import java.time.ZoneId
import java.util.Date

@StringRes
internal fun chatErrorText(key: String?): Int = when (key) {
    "chat_text_empty" -> R.string.chat_error_chat_text_empty
    "chat_text_too_long" -> R.string.chat_error_chat_text_too_long
    "chat_client_id_invalid" -> R.string.chat_error_chat_client_id_invalid
    "chat_reply_not_found" -> R.string.chat_error_chat_reply_not_found
    "chat_not_found" -> R.string.chat_error_chat_not_found
    "chat_not_retryable" -> R.string.chat_error_chat_not_retryable
    "chat_upstream_unavailable" -> R.string.chat_error_chat_upstream_unavailable
    "chat_upstream_failed" -> R.string.chat_error_chat_upstream_failed
    "chat_interrupted" -> R.string.chat_error_chat_interrupted
    "chat_storage_failed" -> R.string.chat_error_chat_storage_failed
    "chat_unauthorized" -> R.string.chat_error_chat_unauthorized
    else -> R.string.chat_error_generic
}

@StringRes
internal fun chatAuthor(role: ChatRole): Int = when (role) {
    ChatRole.USER -> R.string.chat_author_me
    ChatRole.ASSISTANT -> R.string.chat_author_assistant
    ChatRole.SYSTEM -> DesignR.string.ds_product_name
}

internal fun chatTime(context: Context, atMillis: Long): String =
    DateFormat.getTimeFormat(context).format(Date(atMillis))

internal fun chatDay(context: Context, date: LocalDate, zone: ZoneId = ZoneId.systemDefault()): String {
    val today = LocalDate.now(zone)
    val millis = date.atStartOfDay(zone).toInstant().toEpochMilli()
    return when {
        !date.isBefore(today.minusDays(1)) && !date.isAfter(today) -> relativeDay(millis, today, zone)
        else -> DateFormat.getMediumDateFormat(context).format(Date(millis))
    }
}

private fun relativeDay(millis: Long, today: LocalDate, zone: ZoneId): String = DateUtils.getRelativeTimeSpanString(
    millis,
    today.atStartOfDay(zone).toInstant().toEpochMilli(),
    DateUtils.DAY_IN_MILLIS,
).toString()

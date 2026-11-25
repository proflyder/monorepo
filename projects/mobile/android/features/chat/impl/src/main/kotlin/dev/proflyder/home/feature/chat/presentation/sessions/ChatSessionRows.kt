package dev.proflyder.home.feature.chat.presentation.sessions

import android.content.Context
import dev.proflyder.home.core.model.ChatKind
import dev.proflyder.home.core.model.ChatPreview
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatSession
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.feature.chat.impl.R

internal sealed interface SessionRow {
    val key: String

    data class Archive(val count: Int) : SessionRow {
        override val key: String = "archive"
    }

    data object Gap : SessionRow {
        override val key: String = "gap"
    }

    data class Item(val session: ChatSession) : SessionRow {
        override val key: String = session.id
    }
}

internal object SessionRows {
    fun build(state: ChatSessionsUiState): List<SessionRow> {
        val items = state.visible.map(SessionRow::Item)
        val archive = state.archivedCount.takeIf { !state.archived && it > 0 && state.query.isBlank() }
        return when (archive) {
            null -> items
            else -> listOf(SessionRow.Archive(archive), SessionRow.Gap) + items
        }
    }
}

internal fun ChatPreview?.describe(context: Context): String {
    val preview = this ?: return context.getString(R.string.chat_session_preview_empty)
    val body = preview.body(context)
    return when (preview.role) {
        ChatRole.USER -> context.getString(R.string.chat_session_preview_me, body)
        ChatRole.ASSISTANT, ChatRole.SYSTEM -> body
    }
}

private fun ChatPreview.body(context: Context): String = when {
    role == ChatRole.ASSISTANT && (status == ChatStatus.PENDING || status == ChatStatus.STREAMING) && text.isEmpty() ->
        context.getString(R.string.chat_session_preview_typing)
    role == ChatRole.ASSISTANT && status == ChatStatus.FAILED && text.isEmpty() -> context.getString(R.string.chat_session_preview_failed)
    text.isNotBlank() -> text.lineSequence().first { it.isNotBlank() }
    else -> context.getString(kindLabel(kind))
}

private fun kindLabel(kind: ChatKind): Int = when (kind) {
    ChatKind.VOICE -> R.string.chat_session_preview_voice
    ChatKind.IMAGE -> R.string.chat_session_preview_image
    ChatKind.FILE -> R.string.chat_session_preview_file
    ChatKind.ALERT -> R.string.chat_session_preview_alert
    ChatKind.TEXT -> R.string.chat_session_preview_empty
}

internal fun ChatSession.initial(context: Context): String =
    displayTitle(context).trim().firstOrNull()?.uppercaseChar()?.toString().orEmpty()

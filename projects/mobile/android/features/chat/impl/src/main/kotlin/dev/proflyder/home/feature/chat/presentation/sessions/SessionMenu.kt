package dev.proflyder.home.feature.chat.presentation.sessions

import android.content.Context
import android.view.LayoutInflater
import android.view.View
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.designsystem.HomePopupMenu
import dev.proflyder.home.core.model.ChatSession
import dev.proflyder.home.feature.chat.data.SessionAction
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.impl.databinding.DialogChatRenameBinding

internal fun ChatSession.displayTitle(context: Context): String =
    title ?: context.getString(R.string.chat_session_untitled)

internal class SessionMenu(
    private val anchor: View,
    private val session: ChatSession,
    private val onAction: (SessionAction) -> Unit,
) {
    private val context: Context get() = anchor.context

    fun show(gravity: Int) {
        val popup = HomePopupMenu(anchor, R.menu.chat_session_actions, ::onItem)
        with(popup.menu) {
            findItem(R.id.chat_session_pin).isVisible = !session.pinned
            findItem(R.id.chat_session_unpin).isVisible = session.pinned
            findItem(R.id.chat_session_archive).isVisible = !session.archived
            findItem(R.id.chat_session_unarchive).isVisible = session.archived
        }
        popup.show(gravity)
    }

    private fun onItem(itemId: Int) {
        when (itemId) {
            R.id.chat_session_rename -> rename()
            R.id.chat_session_pin -> onAction(SessionAction.Pin(session.id, pinned = true))
            R.id.chat_session_unpin -> onAction(SessionAction.Pin(session.id, pinned = false))
            R.id.chat_session_archive -> onAction(SessionAction.Archive(session.id, archived = true))
            R.id.chat_session_unarchive -> onAction(SessionAction.Archive(session.id, archived = false))
            R.id.chat_session_delete -> confirmDelete()
            else -> Unit
        }
    }

    private fun rename() {
        val views = DialogChatRenameBinding.inflate(LayoutInflater.from(context))
        views.chatRenameInput.setText(session.title.orEmpty())
        views.chatRenameInput.selectAll()
        MaterialAlertDialogBuilder(context)
            .setTitle(R.string.chat_session_rename)
            .setView(views.root)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ -> submitTitle(views.chatRenameInput.text?.toString().orEmpty()) }
            .show()
        views.chatRenameInput.requestFocus()
    }

    private fun submitTitle(title: String) {
        if (title.isNotBlank()) onAction(SessionAction.Rename(session.id, title))
    }

    private fun confirmDelete() {
        MaterialAlertDialogBuilder(context)
            .setTitle(R.string.chat_session_delete_title)
            .setMessage(context.getString(R.string.chat_session_delete_message, session.displayTitle(context)))
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(R.string.chat_session_delete) { _, _ -> onAction(SessionAction.Delete(session.id)) }
            .show()
    }
}

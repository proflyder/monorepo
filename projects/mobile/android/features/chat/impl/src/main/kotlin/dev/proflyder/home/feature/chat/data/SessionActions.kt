package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatSessionPatch
import dev.proflyder.home.core.model.Outcome

internal sealed interface SessionAction {
    data class Rename(val id: String, val title: String) : SessionAction
    data class Pin(val id: String, val pinned: Boolean) : SessionAction
    data class Archive(val id: String, val archived: Boolean) : SessionAction
    data class Delete(val id: String) : SessionAction
}

internal class SessionActions(private val sessions: ChatSessionsRepository, private val forget: (String) -> Unit) {
    suspend fun perform(action: SessionAction): Outcome<Unit> = when (action) {
        is SessionAction.Rename -> patch(action.id, ChatSessionPatch(title = action.title.trim()))
        is SessionAction.Pin -> patch(action.id, ChatSessionPatch(pinned = action.pinned))
        is SessionAction.Archive -> patch(action.id, ChatSessionPatch(archived = action.archived))
        is SessionAction.Delete -> sessions.delete(action.id).also { if (it is Outcome.Success) forget(action.id) }
    }

    private suspend fun patch(id: String, patch: ChatSessionPatch): Outcome<Unit> = when (val result = sessions.update(id, patch)) {
        is Outcome.Success -> Outcome.Success(Unit)
        is Outcome.Failure -> result
    }
}

package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedState
import dev.proflyder.home.core.data.PagingAction
import dev.proflyder.home.core.model.Keyed
import kotlinx.coroutines.flow.StateFlow

class PagingHandler<T : Keyed>(
    private val list: PagedList<T>,
    private val undo: UndoableRemoval<T>,
) {
    val state: StateFlow<PagedState<T>> = list.state

    val pendingUndo: StateFlow<T?> = undo.pending

    fun dispatch(intent: PagingIntent) {
        list.dispatch(
            when (intent) {
                is PagingIntent.Refresh -> PagingAction.Refresh(intent.mode)
                PagingIntent.LoadMore -> PagingAction.LoadMore
                PagingIntent.Retry -> PagingAction.Retry
            },
        )
    }

    fun dispatch(intent: ListItemIntent) {
        when (intent) {
            is ListItemIntent.Remove -> state.value.item(intent.key)?.let(undo::remove)
            ListItemIntent.Undo -> undo.undo()
        }
    }

    fun close() = undo.close()
}

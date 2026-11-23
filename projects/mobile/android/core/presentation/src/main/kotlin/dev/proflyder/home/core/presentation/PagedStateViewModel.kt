package dev.proflyder.home.core.presentation

import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedState
import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope

interface PagedScreenState<T : Keyed> {
    val pages: PagedState<T>

    val pendingRemoval: T?
}

abstract class PagedStateViewModel<T : Keyed, S : PagedScreenState<T>, I : Any, E : Any>(
    initialState: S,
    list: PagedList<T>,
    appScope: CoroutineScope,
    commitRemoval: suspend (T) -> Outcome<Unit>,
) : StateViewModel<S, I, E>(initialState) {
    private val undo = UndoableRemoval(list, viewModelScope, appScope, commitRemoval, ::dispatchFailure)
    private val paging = PagingHandler(list, undo)

    init {
        launch { paging.state.collect { pages -> updateState { copyPaging(pages, pendingRemoval) } } }
        launch { paging.pendingUndo.collect { item -> updateState { copyPaging(pages, item) } } }
    }

    fun dispatch(intent: PagingIntent) = paging.dispatch(intent)

    fun dispatch(intent: ListItemIntent) = paging.dispatch(intent)

    override fun onCleared() {
        paging.close()
    }

    protected abstract fun copyPaging(pages: PagedState<T>, pendingRemoval: T?): S

    protected abstract fun pagingFailure(failure: RemoteFailure): E

    private fun dispatchFailure(failure: RemoteFailure) {
        emit(pagingFailure(failure))
    }
}

package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.Pager
import dev.proflyder.home.core.data.PagingState
import dev.proflyder.home.core.data.RefreshMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

class PagingDelegate<T : Any>(
    val pager: Pager<T>,
    private val scope: CoroutineScope,
) {
    val state: StateFlow<PagingState<T>> get() = pager.state

    fun follow(onChange: suspend (PagingState<T>) -> Unit): Job =
        scope.launch { pager.state.collect(onChange) }

    fun refresh(mode: RefreshMode = RefreshMode.SOFT) = pager.refresh(mode)

    fun loadMore() = pager.loadMore()

    fun retry() {
        if (pager.state.value.hasItems) pager.loadMore() else pager.refresh(RefreshMode.HARD)
    }
}

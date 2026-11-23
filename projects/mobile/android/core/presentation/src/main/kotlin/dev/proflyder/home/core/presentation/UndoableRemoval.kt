package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlin.time.Duration
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

class UndoableRemoval<T : Keyed>(
    private val list: PagedList<T>,
    timerScope: CoroutineScope,
    private val commitScope: CoroutineScope,
    private val commit: suspend (T) -> Outcome<Unit>,
    private val onFailure: (RemoteFailure) -> Unit,
    duration: Duration = DEFAULT_UNDO_WINDOW,
) {
    private val window = UndoWindow<T>(timerScope, duration, ::expired)

    val pending: StateFlow<T?> = window.pending

    fun remove(item: T) {
        window.take()?.let(::commitNow)
        list.hide(item.key)
        window.open(item)
    }

    fun undo() {
        window.take()?.let { list.reveal(it.key) }
    }

    fun close() {
        window.take()?.let(::commitNow)
    }

    private fun expired(item: T) = commitNow(item)

    private fun commitNow(item: T) {
        commitScope.launch {
            when (val result = commit(item)) {
                is Outcome.Success -> Unit
                is Outcome.Failure -> {
                    list.reveal(item.key)
                    onFailure(result.failure)
                }
            }
        }
    }
}

package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.model.Keyed
import kotlin.time.Duration
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

class UndoableRemoval<T : Keyed>(
    private val list: PagedList<T>,
    timerScope: CoroutineScope,
    private val commitScope: CoroutineScope,
    private val commit: suspend (T) -> Unit,
    duration: Duration = DEFAULT_UNDO_WINDOW,
) {
    private val window = UndoWindow<T>(timerScope, duration, ::expired)
    private var current: T? = null

    val pending: StateFlow<T?> = window.pending

    fun remove(item: T) {
        current?.let(::commitNow)
        current = item
        list.hide(item.key)
        window.open(item)
    }

    fun undo() {
        val item = current ?: return
        current = null
        window.take()
        list.reveal(item.key)
    }

    fun close() {
        val item = current ?: return
        current = null
        window.take()
        commitNow(item)
    }

    private fun expired(item: T) {
        if (current?.key == item.key) current = null
        commitNow(item)
    }

    private fun commitNow(item: T) {
        commitScope.launch { commit(item) }
    }
}

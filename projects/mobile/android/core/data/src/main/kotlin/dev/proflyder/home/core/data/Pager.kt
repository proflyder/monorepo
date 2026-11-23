package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

const val DEFAULT_PAGE_SIZE: Int = 20

data class Page<T : Any>(val items: List<T>, val nextCursor: String?)

fun interface PageSource<T : Any> {
    suspend fun load(cursor: String?, limit: Int): Outcome<Page<T>>
}

data class PagingState<T : Any>(
    val items: List<T> = emptyList(),
    val refreshing: Boolean = true,
    val appending: Boolean = false,
    val failure: RemoteFailure? = null,
    val appendFailure: RemoteFailure? = null,
    val endReached: Boolean = false,
) {
    val loading: Boolean get() = refreshing || appending

    val hasItems: Boolean get() = items.isNotEmpty()

    val empty: Boolean get() = items.isEmpty() && !refreshing && failure == null
}

data class PagerRemoval<T : Any>(val index: Int, val item: T)

class Pager<T : Any>(
    private val scope: CoroutineScope,
    private val source: PageSource<T>,
    private val keyOf: (T) -> String,
    private val pageSize: Int = DEFAULT_PAGE_SIZE,
) : Refreshable {
    private val mutex = Mutex()
    private val mutable = MutableStateFlow(PagingState<T>())
    private var cursor: String? = null
    private var started = false
    private var job: Job? = null

    val state: StateFlow<PagingState<T>> = mutable.asStateFlow()

    override fun refresh(mode: RefreshMode) {
        if (started && mutable.value.loading) return
        started = true
        mutable.update { it.copy(refreshing = true, failure = null, appendFailure = null) }
        start(reset = true, next = null)
    }

    fun loadMore() {
        val current = mutable.value
        if (!canLoadMore(current)) return
        mutable.update { it.copy(appending = true, appendFailure = null) }
        start(reset = false, next = cursor)
    }

    private fun canLoadMore(current: PagingState<T>): Boolean {
        if (!started) return false
        if (current.loading) return false
        if (current.endReached) return false
        return current.items.isNotEmpty()
    }

    fun itemsSnapshot(): List<T> = mutable.value.items

    fun replaceLocal(items: List<T>) {
        mutable.update { it.copy(items = items.distinctBy(keyOf)) }
    }

    fun updateLocal(key: String, transform: (T) -> T) {
        replaceLocal(mutable.value.items.map { if (keyOf(it) == key) transform(it) else it })
    }

    fun removeLocal(key: String): PagerRemoval<T>? {
        val items = mutable.value.items
        val index = items.indexOfFirst { keyOf(it) == key }
        if (index < 0) return null
        replaceLocal(items.filterIndexed { position, _ -> position != index })
        return PagerRemoval(index, items[index])
    }

    fun restoreLocal(removal: PagerRemoval<T>) {
        val items = mutable.value.items.toMutableList()
        items.add(removal.index.coerceIn(0, items.size), removal.item)
        replaceLocal(items)
    }

    suspend fun <R> optimistic(
        change: (List<T>) -> List<T>,
        request: suspend () -> Outcome<R>,
    ): Outcome<R> {
        val before = mutable.value.items
        replaceLocal(change(before))
        val result = runCatching { request() }.getOrElse { failure ->
            replaceLocal(before)
            throw failure
        }
        if (result is Outcome.Failure) replaceLocal(before)
        return result
    }

    private fun start(reset: Boolean, next: String?) {
        job?.cancel()
        job = scope.launch { mutex.withLock { load(reset, next) } }
    }

    private suspend fun load(reset: Boolean, next: String?) {
        when (val result = source.load(next, pageSize)) {
            is Outcome.Success -> applyPage(result.value, reset)
            is Outcome.Failure -> fail(result.failure, reset)
        }
    }

    private fun applyPage(page: Page<T>, reset: Boolean) {
        val existing = if (reset) emptyList() else mutable.value.items
        val merged = (existing + page.items).distinctBy(keyOf)
        cursor = page.nextCursor
        mutable.value = PagingState(
            items = merged,
            refreshing = false,
            appending = false,
            endReached = page.nextCursor == null || page.items.isEmpty(),
        )
    }

    private fun fail(failure: RemoteFailure, reset: Boolean) {
        mutable.update {
            if (reset) it.copy(refreshing = false, failure = failure) else it.copy(appending = false, appendFailure = failure)
        }
    }
}

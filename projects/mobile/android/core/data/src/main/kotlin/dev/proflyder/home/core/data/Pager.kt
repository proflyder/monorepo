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

    fun reload() {
        if (mutable.value.loading) return
        mutable.update { it.copy(refreshing = true, failure = null, appendFailure = null) }
        started = true
        job?.cancel()
        job = scope.launch { mutex.withLock { loadFirstPage() } }
    }

    fun loadMore() {
        val current = mutable.value
        if (!canLoadMore(current)) return
        mutable.update { it.copy(appending = true, appendFailure = null) }
        start(reset = false, next = cursor)
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
        if (mutable.value.items.any { keyOf(it) == keyOf(removal.item) }) return
        val items = mutable.value.items.toMutableList()
        items.add(removal.index.coerceIn(0, items.size), removal.item)
        replaceLocal(items)
    }

    suspend fun <R> optimisticUpdate(
        key: String,
        transform: (T) -> T,
        request: suspend () -> Outcome<R>,
    ): Outcome<R> {
        val previous = mutable.value.items.firstOrNull { keyOf(it) == key }
        val optimistic = previous?.let(transform)
        if (optimistic != null) updateLocal(key, transform)
        val result = runCatching { request() }.getOrElse { failure ->
            rollbackUpdate(key, previous, optimistic)
            throw failure
        }
        if (result is Outcome.Failure) rollbackUpdate(key, previous, optimistic)
        return result
    }

    suspend fun <R> optimisticRemove(
        key: String,
        request: suspend () -> Outcome<R>,
    ): Outcome<R> {
        val removal = removeLocal(key)
        val result = runCatching { request() }.getOrElse { failure ->
            rollbackRemoval(removal)
            throw failure
        }
        if (result is Outcome.Failure) rollbackRemoval(removal)
        return result
    }

    suspend fun <R> optimisticEach(
        transform: (T) -> T,
        request: suspend () -> Outcome<R>,
    ): Outcome<R> {
        val before = mutable.value.items.associateBy(keyOf)
        val optimistic = before.mapValues { (_, value) -> transform(value) }
        replaceLocal(mutable.value.items.map(transform))
        val result = runCatching { request() }.getOrElse { failure ->
            restoreEach(before, optimistic)
            throw failure
        }
        if (result is Outcome.Failure) restoreEach(before, optimistic)
        return result
    }

    private fun rollbackUpdate(key: String, previous: T?, optimistic: T?) {
        if (previous == null || optimistic == null) return
        replaceLocal(mutable.value.items.map { if (keyOf(it) == key && it == optimistic) previous else it })
    }

    private fun rollbackRemoval(removal: PagerRemoval<T>?) {
        if (removal == null) return
        if (mutable.value.items.any { keyOf(it) == keyOf(removal.item) }) return
        restoreLocal(removal)
    }

    private fun restoreEach(before: Map<String, T>, optimistic: Map<String, T>) {
        replaceLocal(
            mutable.value.items.map { current ->
                val key = keyOf(current)
                val previous = before[key]
                when {
                    previous == null -> current
                    current != optimistic[key] -> current
                    else -> previous
                }
            },
        )
    }

    private fun canLoadMore(current: PagingState<T>): Boolean {
        if (!started) return false
        if (current.loading) return false
        if (current.endReached) return false
        return current.items.isNotEmpty()
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

    private suspend fun loadFirstPage() {
        when (val result = source.load(null, pageSize)) {
            is Outcome.Success -> mergeFirstPage(result.value)
            is Outcome.Failure -> fail(result.failure, reset = true)
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
            endReached = page.nextCursor == null,
        )
    }

    private fun mergeFirstPage(page: Page<T>) {
        val incoming = page.items.distinctBy(keyOf)
        val keys = incoming.mapTo(HashSet(), keyOf)
        val older = mutable.value.items.filterNot { keyOf(it) in keys }
        cursor = page.nextCursor ?: cursor
        mutable.value = PagingState(
            items = incoming + older,
            refreshing = false,
            appending = false,
            endReached = page.nextCursor == null,
        )
    }

    private fun fail(failure: RemoteFailure, reset: Boolean) {
        mutable.update {
            if (reset) it.copy(refreshing = false, failure = failure) else it.copy(appending = false, appendFailure = failure)
        }
    }
}

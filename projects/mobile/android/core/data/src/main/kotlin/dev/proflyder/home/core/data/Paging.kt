package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

const val DEFAULT_PAGE_SIZE: Int = 20

data class PageRequest(val cursor: String?, val size: Int)

data class Page<T : Keyed>(val items: List<T>, val next: String?)

fun interface PagedSource<T : Keyed> {
    suspend fun load(request: PageRequest): Outcome<Page<T>>
}

sealed interface LoadState {
    data object Idle : LoadState
    data object Loading : LoadState
    data object End : LoadState
    data class Failed(val failure: RemoteFailure) : LoadState
}

data class PagedState<T : Keyed>(
    val items: List<T> = emptyList(),
    val refresh: LoadState = LoadState.Idle,
    val append: LoadState = LoadState.Idle,
) {
    val isEmpty: Boolean get() = items.isEmpty()

    val isInitialLoading: Boolean get() = items.isEmpty() && refresh == LoadState.Loading

    val isRefreshing: Boolean get() = items.isNotEmpty() && refresh == LoadState.Loading

    fun item(key: String): T? = items.firstOrNull { it.key == key }
}

sealed interface PagingAction {
    data class Refresh(val mode: RefreshMode) : PagingAction
    data object Reload : PagingAction
    data object LoadMore : PagingAction
    data object Retry : PagingAction
}

enum class LoadMode { Reset, Append, Merge }

sealed interface PagingEffect {
    data class Load(val request: PageRequest, val mode: LoadMode) : PagingEffect
}

data class PagingTransition<T : Keyed>(val state: PagedState<T>, val effect: PagingEffect.Load? = null)

class PagingReducer(private val pageSize: Int = DEFAULT_PAGE_SIZE) {
    fun <T : Keyed> start(state: PagedState<T>, action: PagingAction, next: String?): PagingTransition<T> =
        when (action) {
            is PagingAction.Refresh -> refresh(state)
            PagingAction.Reload -> reload(state)
            PagingAction.LoadMore -> loadMore(state, next)
            PagingAction.Retry -> retry(state, next)
        }

    fun <T : Keyed> loaded(state: PagedState<T>, page: Page<T>, mode: LoadMode): PagedState<T> =
        when (mode) {
            LoadMode.Reset -> reset(state, page)
            LoadMode.Append -> append(state, page)
            LoadMode.Merge -> merge(state, page)
        }

    fun <T : Keyed> failed(state: PagedState<T>, failure: RemoteFailure, mode: LoadMode): PagedState<T> =
        when (mode) {
            LoadMode.Reset, LoadMode.Merge -> state.copy(refresh = LoadState.Failed(failure), append = LoadState.Idle)
            LoadMode.Append -> state.copy(append = LoadState.Failed(failure))
        }

    private fun <T : Keyed> refresh(state: PagedState<T>): PagingTransition<T> {
        if (state.refresh == LoadState.Loading) return PagingTransition(state)
        val next = state.copy(refresh = LoadState.Loading, append = LoadState.Idle)
        return PagingTransition(next, PagingEffect.Load(PageRequest(null, pageSize), LoadMode.Reset))
    }

    private fun <T : Keyed> reload(state: PagedState<T>): PagingTransition<T> {
        if (state.refresh == LoadState.Loading || state.append == LoadState.Loading) return PagingTransition(state)
        val next = state.copy(refresh = LoadState.Loading)
        return PagingTransition(next, PagingEffect.Load(PageRequest(null, pageSize), LoadMode.Merge))
    }

    private fun <T : Keyed> loadMore(state: PagedState<T>, cursor: String?): PagingTransition<T> {
        if (state.refresh == LoadState.Loading) return PagingTransition(state)
        if (state.append == LoadState.Loading || state.append == LoadState.End) return PagingTransition(state)
        if (state.items.isEmpty()) return PagingTransition(state)
        val next = state.copy(append = LoadState.Loading)
        return PagingTransition(next, PagingEffect.Load(PageRequest(cursor, pageSize), LoadMode.Append))
    }

    private fun <T : Keyed> retry(state: PagedState<T>, cursor: String?): PagingTransition<T> {
        if (state.refresh is LoadState.Failed) return refresh(state)
        if (state.append is LoadState.Failed) {
            val next = state.copy(append = LoadState.Loading)
            return PagingTransition(next, PagingEffect.Load(PageRequest(cursor, pageSize), LoadMode.Append))
        }
        return PagingTransition(state)
    }

    private fun <T : Keyed> reset(state: PagedState<T>, page: Page<T>): PagedState<T> = state.copy(
        items = page.items.distinctBy(Keyed::key),
        refresh = page.endState(),
        append = LoadState.Idle,
    )

    private fun <T : Keyed> append(state: PagedState<T>, page: Page<T>): PagedState<T> = state.copy(
        items = (state.items + page.items).distinctBy(Keyed::key),
        append = page.endState(),
    )

    private fun <T : Keyed> merge(state: PagedState<T>, page: Page<T>): PagedState<T> {
        val incoming = page.items.distinctBy(Keyed::key)
        val keys = incoming.mapTo(HashSet(), Keyed::key)
        val older = state.items.filterNot { it.key in keys }
        return state.copy(
            items = incoming + older,
            refresh = page.endState(),
            append = if (page.next == null) LoadState.End else LoadState.Idle,
        )
    }

    private fun <T : Keyed> Page<T>.endState(): LoadState = if (next == null) LoadState.End else LoadState.Idle
}

sealed interface ListEdit<T : Keyed> {
    data class Update<T : Keyed>(val key: String, val transform: (T) -> T) : ListEdit<T>

    data class Remove<T : Keyed>(val key: String) : ListEdit<T>

    data class UpdateAll<T : Keyed>(val transform: (T) -> T) : ListEdit<T>
}

class PagedList<T : Keyed>(
    private val scope: CoroutineScope,
    private val source: PagedSource<T>,
    private val pageSize: Int = DEFAULT_PAGE_SIZE,
) {
    private val lock = Any()
    private val reducer = PagingReducer(pageSize)
    private val mutex = Mutex()
    private val mutable = MutableStateFlow(PagedState<T>())
    private val hidden = LinkedHashMap<String, HiddenItem<T>>()
    private var next: String? = null
    private var job: Job? = null

    val state: StateFlow<PagedState<T>> = mutable.asStateFlow()

    fun dispatch(action: PagingAction) {
        val transition = synchronized(lock) { reducer.start(mutable.value, action, next) }
        mutable.value = transition.state
        val effect = transition.effect ?: return
        job?.cancel()
        job = scope.launch { mutex.withLock { load(effect) } }
    }

    fun hide(key: String): Boolean = synchronized(lock) { hideLocked(key) }

    fun reveal(key: String) {
        synchronized(lock) { revealLocked(key) }
    }

    private fun hideLocked(key: String): Boolean {
        if (hidden.contains(key)) return true
        val items = mutable.value.items
        val index = items.indexOfFirst { it.key == key }
        if (index < 0) return false
        hidden[key] = HiddenItem(index, items[index])
        mutable.value = mutable.value.copy(items = items.filterIndexed { position, _ -> position != index })
        return true
    }

    private fun revealLocked(key: String) {
        val entry = hidden.remove(key) ?: return
        if (mutable.value.items.any { it.key == key }) return
        val items = mutable.value.items.toMutableList()
        items.add(entry.index.coerceIn(0, items.size), entry.item)
        mutable.value = mutable.value.copy(items = items)
    }

    suspend fun <R> commit(edit: ListEdit<T>, request: suspend () -> Outcome<R>): Outcome<R> {
        val backup = synchronized(lock) { apply(edit) }
        val result = runCatching { request() }.getOrElse { failure ->
            synchronized(lock) { rollback(backup) }
            throw failure
        }
        synchronized(lock) {
            if (result is Outcome.Failure) rollback(backup) else settle(edit)
        }
        return result
    }

    private suspend fun load(effect: PagingEffect.Load) {
        when (val result = source.load(effect.request)) {
            is Outcome.Success -> applyPage(result.value, effect.mode)
            is Outcome.Failure -> applyFailure(result.failure, effect.mode)
        }
    }

    private fun applyPage(page: Page<T>, mode: LoadMode) {
        synchronized(lock) {
            val visible = page.copy(items = page.items.filterNot { hidden.containsKey(it.key) })
            next = page.next
            mutable.value = reducer.loaded(mutable.value, visible, mode)
        }
    }

    private fun applyFailure(failure: RemoteFailure, mode: LoadMode) {
        synchronized(lock) {
            mutable.value = reducer.failed(mutable.value, failure, mode)
        }
    }

    private fun apply(edit: ListEdit<T>): Backup<T> = when (edit) {
        is ListEdit.Update -> update(edit.key, edit.transform)
        is ListEdit.Remove -> remove(edit.key)
        is ListEdit.UpdateAll -> updateAll(edit.transform)
    }

    private fun update(key: String, transform: (T) -> T): Backup<T> {
        val previous = mutable.value.item(key) ?: return Backup.Update(key, null, null)
        val optimistic = transform(previous)
        mutable.value = mutable.value.copy(
            items = mutable.value.items.map { if (it.key == key) optimistic else it },
        )
        return Backup.Update(key, previous, optimistic)
    }

    private fun remove(key: String): Backup<T> {
        hide(key)
        return Backup.Remove(key)
    }

    private fun updateAll(transform: (T) -> T): Backup<T> {
        val before = mutable.value.items.associateBy(Keyed::key)
        val optimistic = before.mapValues { (_, item) -> transform(item) }
        mutable.value = mutable.value.copy(items = mutable.value.items.map(transform))
        return Backup.All(before, optimistic)
    }

    private fun settle(edit: ListEdit<T>) {
        if (edit is ListEdit.Remove) hidden.remove(edit.key)
    }

    private fun rollback(backup: Backup<T>) {
        when (backup) {
            is Backup.Update -> rollbackUpdate(backup)
            is Backup.Remove -> reveal(backup.key)
            is Backup.All -> rollbackAll(backup)
        }
    }

    private fun rollbackUpdate(backup: Backup.Update<T>) {
        val previous = backup.previous ?: return
        val optimistic = backup.optimistic ?: return
        mutable.value = mutable.value.copy(
            items = mutable.value.items.map { if (it.key == backup.key && it == optimistic) previous else it },
        )
    }

    private fun rollbackAll(backup: Backup.All<T>) {
        mutable.value = mutable.value.copy(
            items = mutable.value.items.map { current ->
                val previous = backup.before[current.key]
                if (previous != null && current == backup.optimistic[current.key]) previous else current
            },
        )
    }

    private sealed interface Backup<T : Keyed> {
        data class Update<T : Keyed>(val key: String, val previous: T?, val optimistic: T?) : Backup<T>
        data class Remove<T : Keyed>(val key: String) : Backup<T>
        data class All<T : Keyed>(val before: Map<String, T>, val optimistic: Map<String, T>) : Backup<T>
    }

    private data class HiddenItem<T : Keyed>(val index: Int, val item: T)
}

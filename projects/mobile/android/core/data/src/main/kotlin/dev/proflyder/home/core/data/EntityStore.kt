package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

interface EntitySource<K : Any, V : Any> : Refreshable {
    val entities: StateFlow<Map<K, V>>
    val items: StateFlow<List<V>>
    val load: StateFlow<ListLoadState>

    fun observe(key: K): Flow<V?> = entities.map { it[key] }.distinctUntilChanged()

    operator fun get(key: K): V? = entities.value[key]
}

typealias EntityChange<K, V> = (Map<K, V>) -> Map<K, V>

class EntityStore<K : Any, V : Any>(
    private val scope: CoroutineScope,
    private val keyOf: (V) -> K,
    private val order: Comparator<in V>? = null,
    private val fetch: suspend () -> Outcome<List<V>>,
) : EntitySource<K, V> {
    private val lock = Any()
    private var confirmed: Map<K, V> = emptyMap()
    private val overlays = mutableListOf<Overlay<K, V>>()
    private val writes = WriteJournal<K>()
    private var syncing: Job? = null

    private val mutableEntities = MutableStateFlow<Map<K, V>>(emptyMap())
    private val mutableItems = MutableStateFlow<List<V>>(emptyList())
    private val mutableLoad = MutableStateFlow(ListLoadState())

    override val entities: StateFlow<Map<K, V>> = mutableEntities.asStateFlow()
    override val items: StateFlow<List<V>> = mutableItems.asStateFlow()
    override val load: StateFlow<ListLoadState> = mutableLoad.asStateFlow()

    override fun refresh(mode: RefreshMode) {
        if (mode == RefreshMode.SOFT && syncing?.isActive == true) return
        mutableLoad.update { it.after(LoadStep.Started(mode)) }
        syncing?.cancel()
        syncing = scope.launch { sync(writes.mark()) }
    }

    fun upsert(value: V) = write { confirmed + (keyOf(value) to value) to listOf(keyOf(value)) }

    fun remove(key: K) = write { confirmed - key to listOf(key) }

    fun replaceAll(values: List<V>) {
        synchronized(lock) { replaceLocked(values) }
    }

    private fun replaceLocked(values: List<V>) {
        confirmed = values.associateByTo(LinkedHashMap(), keyOf)
        writes.clear()
        publish()
    }

    suspend fun <T> commit(request: suspend () -> Outcome<T>, settle: EntityStore<K, V>.(T) -> Unit): Outcome<T> =
        request().also { result -> if (result is Outcome.Success) settle(result.value) }

    suspend fun <T> optimistic(
        change: EntityChange<K, V>,
        request: suspend () -> Outcome<T>,
        settle: EntityStore<K, V>.(T) -> Unit,
    ): Outcome<T> {
        val overlay = Overlay(change)
        synchronized(lock) { addOverlay(overlay) }
        val result = runCatching { request() }
        synchronized(lock) { settleOverlay(overlay, result.getOrNull(), settle) }
        return result.getOrThrow()
    }

    private suspend fun sync(since: Long) {
        when (val result = fetch()) {
            is Outcome.Success -> merge(result.value, since)
            is Outcome.Failure -> mutableLoad.update { it.after(LoadStep.Failed(result.failure)) }
        }
    }

    private fun merge(fetched: List<V>, since: Long) {
        synchronized(lock) { mergeLocked(fetched, since) }
        mutableLoad.update { it.after(LoadStep.Loaded(Unit)) }
    }

    private fun mergeLocked(fetched: List<V>, since: Long) {
        val touched = writes.after(since)
        val merged = LinkedHashMap<K, V>()
        fetched.filterNot { keyOf(it) in touched }.forEach { merged[keyOf(it)] = it }
        touched.forEach { key -> confirmed[key]?.let { merged[key] = it } }
        confirmed = merged
        writes.forget(since)
        publish()
    }

    private fun write(change: () -> Pair<Map<K, V>, List<K>>) {
        synchronized(lock) { writeLocked(change()) }
    }

    private fun writeLocked(change: Pair<Map<K, V>, List<K>>) {
        confirmed = change.first
        change.second.forEach(writes::touch)
        publish()
    }

    private fun addOverlay(overlay: Overlay<K, V>) {
        overlays += overlay
        publish()
    }

    private fun <T> settleOverlay(overlay: Overlay<K, V>, outcome: Outcome<T>?, settle: EntityStore<K, V>.(T) -> Unit) {
        overlays -= overlay
        if (outcome is Outcome.Success) settle(outcome.value)
        publish()
    }

    private fun publish() {
        val visible = overlays.fold(confirmed) { current, overlay -> overlay.change(current) }
        val sorted = order?.let { visible.values.sortedWith(it) } ?: visible.values.toList()
        mutableEntities.value = sorted.associateByTo(LinkedHashMap(), keyOf)
        mutableItems.value = sorted
    }

    private class Overlay<K, V>(val change: EntityChange<K, V>)
}

private class WriteJournal<K : Any> {
    private val stamps = mutableMapOf<K, Long>()
    private var sequence = 0L

    fun mark(): Long = sequence

    fun touch(key: K) {
        sequence += 1
        stamps[key] = sequence
    }

    fun after(since: Long): Set<K> = stamps.filterValues { it > since }.keys

    fun forget(until: Long) {
        stamps.entries.removeAll { it.value <= until }
    }

    fun clear() = stamps.clear()
}

fun EntitySource<*, *>.backgroundFailures(): Flow<RemoteFailure> = load
    .map { state -> state.failure?.takeIf { items.value.isNotEmpty() } }
    .distinctUntilChanged()
    .filterNotNull()

fun <V : Any> EntitySource<*, V>.loadedItems(): Flow<List<V>?> = combine(items, load) { values, state ->
    values.takeIf { it.isNotEmpty() || (!state.refreshing && state.failure == null) }
}.distinctUntilChanged()

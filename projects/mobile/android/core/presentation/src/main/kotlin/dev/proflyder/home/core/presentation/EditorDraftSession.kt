package dev.proflyder.home.core.presentation

import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.isAmbiguousRetryable
import dev.proflyder.home.core.operation.IdempotentRequestKey

interface EditorDraftCodec<D : Any> {
    fun encode(draft: D): Map<String, Any?>

    fun decode(values: Map<String, Any?>): D?
}

data class EditorStatus(
    val loading: Boolean = false,
    val saving: Boolean = false,
    val loadFailed: Boolean = false,
    val error: RemoteFailure? = null,
) {
    val busy: Boolean get() = loading || saving

    val idle: Boolean get() = !busy && !loadFailed

    fun loadingStarted(): EditorStatus = copy(loading = true, error = null)

    fun loadFailed(failure: RemoteFailure): EditorStatus = copy(loading = false, loadFailed = true, error = failure)

    fun savingStarted(): EditorStatus = copy(saving = true, error = null)

    fun settled(failure: RemoteFailure?): EditorStatus = EditorStatus(loadFailed = loadFailed, error = failure)
}

class EditorDraftSession<D : Any, P : Any, C : Enum<C>>(
    private val savedState: SavedStateHandle,
    private val keyPrefix: String,
    private val codec: EditorDraftCodec<D>,
    private val completions: List<C>,
    private val endpoint: () -> String,
) {
    private val identity = IdempotentRequestKey<Pair<P, String>>()

    val restored: D? = codec.decode(savedState.keys().associateWith { savedState.get<Any?>(it) })

    val pendingCompletion: C?
        get() = savedState.get<String>(completionKey)?.let { name -> completions.firstOrNull { it.name == name } }

    private val completionKey get() = "${keyPrefix}_completion"
    private val createKeyKey get() = "${keyPrefix}_create_key"
    private val createEndpointKey get() = "${keyPrefix}_create_endpoint"

    fun adoptRestoredCreate(payload: P) {
        val key = savedState.get<String>(createKeyKey) ?: return
        val current = endpoint()
        if (savedState.get<String>(createEndpointKey) != current) return
        identity.adopt(payload to current, key)
    }

    fun persist(draft: D) {
        codec.encode(draft).forEach { (key, value) -> savedState[key] = value }
    }

    fun edited(draft: D) {
        identity.reset()
        clearPendingKey()
        persist(draft)
    }

    suspend fun <T> submit(createPayload: P?, request: suspend (String) -> Outcome<T>): Outcome<T> {
        val key = createPayload?.let(::createKey).orEmpty()
        if (createPayload == null) clearPendingKey()
        return request(key).also(::settle)
    }

    fun complete(completion: C) {
        savedState[completionKey] = completion.name
    }

    fun consumeCompletion() {
        savedState.remove<String>(completionKey)
    }

    private fun createKey(payload: P): String {
        val current = endpoint()
        return identity.keyFor(payload to current).also {
            savedState[createKeyKey] = it
            savedState[createEndpointKey] = current
        }
    }

    private fun settle(outcome: Outcome<*>) {
        val failure = (outcome as? Outcome.Failure)?.failure
        if (failure?.isAmbiguousRetryable() == true) return
        identity.reset()
        clearPendingKey()
    }

    private fun clearPendingKey() {
        savedState.remove<String>(createKeyKey)
        savedState.remove<String>(createEndpointKey)
    }
}

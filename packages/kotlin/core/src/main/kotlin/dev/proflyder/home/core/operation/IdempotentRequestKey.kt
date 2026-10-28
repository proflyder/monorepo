package dev.proflyder.home.core.operation

/**
 * Retains one non-secret idempotency key for a logical create/start operation.
 *
 * The key stays stable while the same payload is retried after an ambiguous
 * network outcome (the server may have accepted the request but the response
 * was lost). A success, a definite rejection, or an intentional payload change
 * starts a new logical operation with a new key.
 *
 * This class holds no secrets; callers keep it in the ViewModel or other
 * in-memory operation state.
 */
class IdempotentRequestKey<T>(
    private val generate: () -> String = { java.util.UUID.randomUUID().toString() },
) {
    private var pendingPayload: T? = null
    private var pendingKey: String? = null

    @Synchronized
    fun keyFor(payload: T): String {
        val existing = pendingKey
        if (existing != null && pendingPayload == payload) return existing
        return generate().also {
            pendingKey = it
            pendingPayload = payload
        }
    }

    /** Restores a pending key after process recreation for the same payload. */
    @Synchronized
    fun adopt(payload: T, key: String) {
        pendingPayload = payload
        pendingKey = key
    }

    @Synchronized
    fun markSucceeded() {
        pendingKey = null
        pendingPayload = null
    }

    /** Use for definite rejection or when the user changes the intended request. */
    @Synchronized
    fun reset() {
        pendingKey = null
        pendingPayload = null
    }

    @Synchronized
    fun pendingOrNull(): String? = pendingKey
}

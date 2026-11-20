package dev.proflyder.home.core.operation

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

    @Synchronized
    fun reset() {
        pendingKey = null
        pendingPayload = null
    }

    @Synchronized
    fun pendingOrNull(): String? = pendingKey
}

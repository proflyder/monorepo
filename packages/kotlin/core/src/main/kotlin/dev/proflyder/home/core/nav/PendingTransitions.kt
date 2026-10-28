package dev.proflyder.home.core.nav

/**
 * A small, Android-free queue for navigation commands that arrive while the
 * FragmentManager cannot safely commit (state saved or host not resumed).
 *
 * Commands are drained in order and only once. A failing consumer leaves the
 * remaining commands queued so a later resume can retry them.
 */
class PendingTransitions<T>(private val capacity: Int = 32) {
    init {
        require(capacity > 0) { "capacity must be positive" }
    }

    private val items = ArrayDeque<T>()

    val size: Int get() = items.size
    val isEmpty: Boolean get() = items.isEmpty()

    /**
     * Enqueues [item]. Consecutive duplicates are collapsed. Returns false when
     * the bounded queue is full; the caller can surface that as a UI error
     * instead of silently dropping a transition.
     */
    fun offer(item: T): Boolean {
        if (items.isNotEmpty() && items.last() == item) return true
        if (items.size >= capacity) return false
        items.addLast(item)
        return true
    }

    fun drain(consumer: (T) -> Unit) {
        while (items.isNotEmpty()) {
            val next = items.first()
            consumer(next)
            items.removeFirst()
        }
    }

    fun clear() {
        items.clear()
    }

    fun snapshot(): List<T> = items.toList()
}

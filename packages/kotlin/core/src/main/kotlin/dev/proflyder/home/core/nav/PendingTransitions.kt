package dev.proflyder.home.core.nav

class PendingTransitions<T>(private val capacity: Int = 32) {
    init {
        require(capacity > 0) { "capacity must be positive" }
    }

    private val items = ArrayDeque<T>()

    val size: Int get() = items.size
    val isEmpty: Boolean get() = items.isEmpty()

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

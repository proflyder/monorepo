package dev.proflyder.home.core.operation

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * A completion signal that is retained until the UI consumes it.
 *
 * Editor saves/deletes can finish while the Fragment is STOPPED. A replay-0
 * SharedFlow would lose that event; this state survives until the resumed UI
 * consumes it exactly once.
 */
class EditorCompletion<T>(initial: T? = null) {
    private val mutable = MutableStateFlow(initial)
    val state: StateFlow<T?> = mutable.asStateFlow()

    fun publish(value: T) {
        mutable.value = value
    }

    fun consume(): T? {
        val value = mutable.value
        mutable.value = null
        return value
    }
}

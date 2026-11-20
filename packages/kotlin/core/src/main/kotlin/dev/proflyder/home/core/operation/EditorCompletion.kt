package dev.proflyder.home.core.operation

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

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

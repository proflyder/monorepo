package dev.proflyder.home.core.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * Holds immutable UI state of type [S] and exposes it as a read-only [state].
 * State is changed only through [updateState]; reads inside the reducer are
 * atomic with the write.
 */
abstract class BaseViewModel<S : Any>(initialState: S) : ViewModel() {
    /** Backing flow for subclasses that need `compareAndSet` or `value` access. */
    protected val mutableState = MutableStateFlow(initialState)

    val state: StateFlow<S> = mutableState.asStateFlow()

    protected val currentState: S get() = mutableState.value

    protected fun updateState(reducer: S.() -> S) {
        mutableState.update { it.reducer() }
    }

    protected fun launch(block: suspend CoroutineScope.() -> Unit): Job = viewModelScope.launch(block = block)
}

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

abstract class BaseViewModel<S : Any>(initialState: S) : ViewModel() {
    protected val mutableState = MutableStateFlow(initialState)

    val state: StateFlow<S> = mutableState.asStateFlow()

    protected val currentState: S get() = mutableState.value

    protected fun updateState(reducer: S.() -> S) {
        mutableState.update { previous -> derive(previous, previous.reducer()) }
    }

    protected open fun derive(previous: S, next: S): S = next

    protected fun launch(block: suspend CoroutineScope.() -> Unit): Job = viewModelScope.launch(block = block)
}

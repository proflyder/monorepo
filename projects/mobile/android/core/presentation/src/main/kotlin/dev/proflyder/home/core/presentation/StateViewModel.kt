package dev.proflyder.home.core.presentation

import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.ui.BaseViewModel
import kotlin.properties.ReadOnlyProperty
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow

abstract class StateViewModel<S : Any, I : Any, E : Any>(initialState: S) : BaseViewModel<S>(initialState) {
    private val pendingEvents = Channel<E>(Channel.UNLIMITED)

    val events: Flow<E> = pendingEvents.receiveAsFlow()

    fun dispatch(intent: I) {
        handle(intent)
    }

    protected abstract fun handle(intent: I)

    protected fun emit(event: E) {
        pendingEvents.trySend(event)
    }

    protected fun latest(): ReadOnlyProperty<Any?, LatestLauncher> = viewModelScope.latestLauncher()

    override fun onCleared() {
        pendingEvents.close()
    }
}

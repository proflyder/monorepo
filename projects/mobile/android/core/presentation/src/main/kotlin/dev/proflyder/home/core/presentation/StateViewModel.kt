package dev.proflyder.home.core.presentation

import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.ui.BaseViewModel
import kotlin.properties.ReadOnlyProperty
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

abstract class StateViewModel<S : Any, I : Any, E : Any>(initialState: S) : BaseViewModel<S>(initialState) {
    val screenEvents: ScreenEvents<E> = ScreenEvents()

    val events: Flow<E> get() = screenEvents.consumed

    val visibility: ScreenVisibility = ScreenVisibility()

    fun dispatch(intent: I) {
        handle(intent)
    }

    protected abstract fun handle(intent: I)

    protected fun emit(event: E) {
        screenEvents.emit(event)
    }

    protected fun latest(): ReadOnlyProperty<Any?, LatestLauncher> = viewModelScope.latestLauncher()

    protected fun Refreshable.refreshOn(signals: Flow<Any>) {
        signals.onEach { refresh(RefreshMode.SOFT) }.launchIn(viewModelScope)
    }
}

package dev.proflyder.home.core.presentation

import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

class RenderedSlice<T>(private val block: (T) -> Unit) {
    private var rendered = false
    private var last: T? = null

    fun offer(value: T) {
        if (rendered && last == value) return
        rendered = true
        last = value
        block(value)
    }
}

fun <S, T> LifecycleOwner.renderSlice(state: Flow<S>, selector: (S) -> T, block: (T) -> Unit) {
    val slice = RenderedSlice(block)
    lifecycleScope.launch {
        repeatOnLifecycle(Lifecycle.State.STARTED) {
            state.map(selector).distinctUntilChanged().collect(slice::offer)
        }
    }
}

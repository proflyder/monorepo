package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.RemoteFailure

enum class RefreshMode {
    SOFT,
    HARD,
}

interface Refreshable {
    fun refresh(mode: RefreshMode)
}

data class ListLoadState(
    val refreshing: Boolean = true,
    val failure: RemoteFailure? = null,
) {
    fun after(step: LoadStep<*>): ListLoadState = when (step) {
        is LoadStep.Started -> copy(refreshing = refreshing || step.mode == RefreshMode.HARD, failure = null)
        is LoadStep.Loaded -> ListLoadState(refreshing = false)
        is LoadStep.Failed -> ListLoadState(refreshing = false, failure = step.failure)
    }
}

sealed interface LoadStep<out T> {
    data class Started(val mode: RefreshMode) : LoadStep<Nothing>
    data class Loaded<T>(val value: T) : LoadStep<T>
    data class Failed(val failure: RemoteFailure) : LoadStep<Nothing>
}

fun <T> LoadStep<T>.valueOr(current: T): T = (this as? LoadStep.Loaded<T>)?.value ?: current

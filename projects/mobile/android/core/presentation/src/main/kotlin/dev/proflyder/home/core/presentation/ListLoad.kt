package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope

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

class ListRefresher<T>(
    scope: CoroutineScope,
    private val load: suspend () -> Outcome<T>,
    private val onStep: (LoadStep<T>) -> Unit,
) : Refreshable {
    private val launcher = LatestLauncher(scope)

    val isRefreshing: Boolean get() = launcher.isActive

    override fun refresh(mode: RefreshMode) {
        onStep(LoadStep.Started(mode))
        launcher.launch { onStep(load().toStep()) }
    }

    private fun Outcome<T>.toStep(): LoadStep<T> = when (this) {
        is Outcome.Success -> LoadStep.Loaded(value)
        is Outcome.Failure -> LoadStep.Failed(failure)
    }
}

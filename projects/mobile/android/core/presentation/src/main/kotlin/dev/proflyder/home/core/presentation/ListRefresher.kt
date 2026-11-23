package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.data.LoadStep
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.Refreshable
import kotlinx.coroutines.CoroutineScope

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

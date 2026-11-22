package dev.proflyder.home.feature.system.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.system.SystemRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.Job

data class SystemUiState(
    val loading: Boolean = true,
    val snapshot: SystemSnapshot? = null,
    val error: RemoteFailure? = null,
)

@Inject
internal class SystemViewModel(
    private val repository: SystemRepository,
) : BaseViewModel<SystemUiState>(SystemUiState()) {
    private var job: Job? = null

    fun refresh() {
        job?.cancel()
        updateState { copy(loading = true) }
        job = launch { apply(repository.load()) }
    }

    private fun apply(result: Outcome<SystemSnapshot>) = when (result) {
        is Outcome.Success -> updateState { copy(loading = false, snapshot = result.value, error = null) }
        is Outcome.Failure -> updateState { copy(loading = false, error = result.failure) }
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(SystemViewModel::class)
@Inject
class SystemViewModelSource internal constructor(
    private val viewModel: () -> SystemViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

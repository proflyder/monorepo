package dev.proflyder.home.feature.control.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.control.domain.LoadControlCatalog
import dev.proflyder.home.feature.control.domain.RunScenario
import dev.proflyder.home.feature.control.domain.SetDevicePower
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay

/** One-shot message for the screen. */
sealed interface ControlNotice {
    data class ScenarioQueued(val title: String) : ControlNotice
}

data class ControlUiState(
    val configured: Boolean = true,
    val refreshing: Boolean = true,
    val entries: List<ControlEntry> = emptyList(),
    /** False when the server could not read Home Assistant; device states are then unknown. */
    val statesOk: Boolean = true,
    /** Entry ids with a command in flight. */
    val pending: Set<String> = emptySet(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    val notice: ControlNotice? = null,
    val filter: ControlFilter = ControlFilter.All,
)

class ControlViewModel(
    private val loadCatalog: LoadControlCatalog,
    private val runScenario: RunScenario,
    private val setDevicePower: SetDevicePower,
) : BaseViewModel<ControlUiState>(ControlUiState(configured = loadCatalog.isConfigured)) {
    private var refreshJob: Job? = null
    private var generation = 0

    /** [silent] refreshes without the pull-to-refresh spinner (used to reconcile after a command). */
    fun refresh(silent: Boolean = false) {
        if (!loadCatalog.isConfigured) {
            updateState { copy(configured = false, refreshing = false, entries = emptyList(), loadError = null) }
            return
        }
        refreshJob?.cancel()
        val requested = ++generation
        updateState { copy(configured = true, refreshing = refreshing || !silent, loadError = null) }
        refreshJob = launch {
            val result = loadCatalog()
            if (requested != generation) return@launch
            when (result) {
                is Outcome.Success -> updateState {
                    val options = ControlItems.options(result.value.entries)
                    copy(
                        refreshing = false,
                        entries = result.value.entries,
                        statesOk = result.value.statesOk,
                        // A room or category that vanished falls back to everything.
                        filter = if (options.any { it.filter == filter }) filter else ControlFilter.All,
                    )
                }
                is Outcome.Failure -> updateState { copy(refreshing = false, loadError = result.failure) }
            }
        }
    }

    fun run(entry: ControlEntry) {
        if (entry.id in currentState.pending || !entry.available) return
        updateState { copy(pending = pending + entry.id, actionError = null) }
        launch {
            when (val result = runScenario(entry)) {
                is Outcome.Success -> updateState {
                    copy(pending = pending - entry.id, notice = ControlNotice.ScenarioQueued(entry.title))
                }
                is Outcome.Failure -> updateState { copy(pending = pending - entry.id, actionError = result.failure) }
            }
        }
    }

    fun setPower(entry: ControlEntry, on: Boolean) {
        if (entry.id in currentState.pending || !entry.available) return
        val target = if (on) PowerState.ON else PowerState.OFF
        // Optimistic: show the requested state, then reconcile with the real one.
        updateState {
            copy(
                pending = pending + entry.id,
                actionError = null,
                entries = entries.map { if (it.id == entry.id) it.copy(power = target) else it },
            )
        }
        launch {
            when (val result = setDevicePower(entry, on)) {
                is Outcome.Success -> {
                    updateState { copy(pending = pending - entry.id) }
                    // The executor applies the command asynchronously; re-read the real state.
                    delay(RECONCILE_DELAY_MS)
                    refresh(silent = true)
                }
                is Outcome.Failure -> {
                    updateState { copy(pending = pending - entry.id, actionError = result.failure) }
                    refresh(silent = true)
                }
            }
        }
    }

    fun setFilter(filter: ControlFilter) = updateState { copy(filter = filter) }

    fun consumeNotice() = updateState { copy(notice = null) }

    fun consumeActionError() = updateState { copy(actionError = null) }

    fun consumeLoadError() = updateState { copy(loadError = null) }

    private companion object {
        const val RECONCILE_DELAY_MS = 1_500L
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class ControlViewModelFactoryContributor(
    private val loadCatalog: LoadControlCatalog,
    private val runScenario: RunScenario,
    private val setDevicePower: SetDevicePower,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ControlViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        ControlViewModel(loadCatalog, runScenario, setDevicePower)
    }
}

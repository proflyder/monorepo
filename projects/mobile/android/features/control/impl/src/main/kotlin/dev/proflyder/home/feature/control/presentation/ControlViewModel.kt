package dev.proflyder.home.feature.control.presentation

import android.os.SystemClock
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItems
import dev.proflyder.home.feature.control.ControlPreferences
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.feature.control.domain.AdjustLight
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
    private val adjustLight: AdjustLight,
    private val preferences: ControlPreferences,
) : BaseViewModel<ControlUiState>(ControlUiState(configured = loadCatalog.isConfigured)) {
    private var refreshJob: Job? = null
    private var generation = 0
    private var filterChosenByUser = false
    private val expectedPower = HashMap<String, Pair<PowerState, Long>>()

    init {
        // Restore the last selected filter. A tap that happens before this
        // returns wins; an unknown room is reset to All once the catalog loads.
        launch {
            val saved = preferences.loadFilter() ?: return@launch
            // If the catalog is already loaded, validate the stored choice against it.
            if (!filterChosenByUser) updateState { copy(filter = ControlItems.effectiveFilter(entries, saved)) }
        }
    }

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
                    copy(
                        refreshing = false,
                        entries = withExpectations(result.value.entries),
                        statesOk = result.value.statesOk,
                        // A room or category that vanished falls back to everything.
                        filter = ControlItems.effectiveFilter(result.value.entries, filter),
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

    /**
     * The executor applies commands asynchronously, so a refresh right after a
     * tap can still return the old state. The tapped value is therefore kept
     * (see [expectedPower]) until the server confirms it or [EXPECT_TTL_MS] passes;
     * the switch never flickers back, and it is not disabled while the command is in flight.
     */
    fun setPower(entry: ControlEntry, on: Boolean) {
        if (!entry.available) return
        val target = if (on) PowerState.ON else PowerState.OFF
        expectedPower[entry.id] = target to SystemClock.elapsedRealtime() + EXPECT_TTL_MS
        updateState {
            copy(actionError = null, entries = entries.map { if (it.id == entry.id) it.copy(power = target) else it })
        }
        launch {
            when (val result = setDevicePower(entry, on)) {
                is Outcome.Success -> reconcile(entry.id)
                is Outcome.Failure -> {
                    expectedPower.remove(entry.id)
                    updateState { copy(actionError = result.failure) }
                    refresh(silent = true)
                }
            }
        }
    }

    /** Re-reads the catalog until the server shows the expected state (or the wait expires). */
    private suspend fun reconcile(id: String) {
        repeat(RECONCILE_ATTEMPTS) {
            delay(RECONCILE_DELAY_MS)
            if (expectedPower[id] == null) return
            refresh(silent = true)
            refreshJob?.join()
        }
    }

    /** Applies still-unconfirmed taps on top of freshly loaded [entries]. */
    private fun withExpectations(entries: List<ControlEntry>): List<ControlEntry> {
        if (expectedPower.isEmpty()) return entries
        val now = SystemClock.elapsedRealtime()
        return entries.map { entry ->
            val (target, deadline) = expectedPower[entry.id] ?: return@map entry
            when {
                entry.power == target || now >= deadline -> entry.also { expectedPower.remove(entry.id) }
                else -> entry.copy(power = target)
            }
        }
    }

    /**
     * Applies a light adjustment. Unlike [setPower] it does not mark the row busy
     * (the user keeps dragging sliders); the optimistic values are replaced by the
     * real ones once the executor has applied the command.
     */
    fun adjust(entry: ControlEntry, adjust: LightAdjust) {
        if (!entry.available || entry.light == null) return
        expectedPower[entry.id] = PowerState.ON to SystemClock.elapsedRealtime() + EXPECT_TTL_MS
        updateState {
            copy(
                actionError = null,
                entries = entries.map { if (it.id == entry.id) it.withAdjust(adjust) else it },
            )
        }
        launch {
            when (val result = adjustLight(entry, adjust)) {
                is Outcome.Success -> reconcile(entry.id)
                is Outcome.Failure -> {
                    expectedPower.remove(entry.id)
                    updateState { copy(actionError = result.failure) }
                    refresh(silent = true)
                }
            }
        }
    }

    fun setFilter(filter: ControlFilter) {
        filterChosenByUser = true
        updateState { copy(filter = filter) }
        launch { preferences.saveFilter(filter) }
    }

    fun consumeNotice() = updateState { copy(notice = null) }

    fun consumeActionError() = updateState { copy(actionError = null) }

    fun consumeLoadError() = updateState { copy(loadError = null) }

    private companion object {
        const val RECONCILE_DELAY_MS = 1_500L
        const val RECONCILE_ATTEMPTS = 8
        const val EXPECT_TTL_MS = 15_000L
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class ControlViewModelFactoryContributor(
    private val loadCatalog: LoadControlCatalog,
    private val runScenario: RunScenario,
    private val setDevicePower: SetDevicePower,
    private val adjustLight: AdjustLight,
    private val preferences: ControlPreferences,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ControlViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        ControlViewModel(loadCatalog, runScenario, setDevicePower, adjustLight, preferences)
    }
}

/** The entry as it will look once [adjust] is applied: any adjustment also turns the light on. */
internal fun ControlEntry.withAdjust(adjust: LightAdjust): ControlEntry {
    val current = light ?: return this
    return copy(
        power = PowerState.ON,
        light = current.copy(
            brightnessPct = adjust.brightnessPct ?: current.brightnessPct,
            kelvin = adjust.kelvin ?: current.kelvin,
            hue = adjust.hue ?: current.hue,
            saturation = adjust.saturation ?: current.saturation,
        ),
    )
}

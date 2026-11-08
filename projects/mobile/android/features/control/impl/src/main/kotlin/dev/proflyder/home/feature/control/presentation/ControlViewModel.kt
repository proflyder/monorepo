package dev.proflyder.home.feature.control.presentation

import android.os.SystemClock
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Favorites
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItems
import dev.proflyder.home.feature.control.ControlPreferences
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.HubCards
import dev.proflyder.home.feature.control.HubFormat
import dev.proflyder.home.feature.control.SpeakText
import dev.proflyder.home.feature.control.domain.AdjustLight
import dev.proflyder.home.feature.control.domain.LoadControlCatalog
import dev.proflyder.home.feature.control.domain.RunScenario
import dev.proflyder.home.feature.control.domain.SetDevicePower
import dev.proflyder.home.feature.control.domain.SetVoiceProfile
import dev.proflyder.home.feature.control.domain.SpeakOnStation
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.proflyder.home.feature.timers.TimerRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import java.util.UUID
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay

/** One-shot message for the screen. */
sealed interface ControlNotice {
    data class ScenarioQueued(val title: String) : ControlNotice

    /** A shortcut was pinned or unpinned from a long press. */
    data class FavoriteChanged(val title: String, val pinned: Boolean) : ControlNotice
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
    /** Device ids whose "Settings (N)" row is open; UI state only, not persisted. */
    val expandedDevices: Set<String> = emptySet(),
    /** Room section ids the user collapsed. */
    val collapsedRooms: Set<String> = emptySet(),
    /** Live carousel cards ("nothing right now" is an empty list). */
    val hubCards: List<HubCard> = emptyList(),
    /** Server-accurate timer seconds per timer card key, refreshed by [ControlViewModel.tick]. */
    val hubRemaining: Map<String, Long> = emptyMap(),
    /** Native scenarios, used for pinned scenario tiles and their busy state. */
    val scenarios: List<Scenario> = emptyList(),
    /** Ordered pinned shortcuts. */
    val favorites: List<FavoriteRef> = emptyList(),
    val pendingScenarios: Set<String> = emptySet(),
    val pendingTimers: Set<String> = emptySet(),
    val pendingAlarms: Set<String> = emptySet(),
    /** Forces one render per countdown tick. */
    val tick: Long = 0,
    /** Server time captured with the hub data, used for "today/tomorrow" text. */
    val nowUnix: Long = 0,
)

class ControlViewModel(
    private val loadCatalog: LoadControlCatalog,
    private val runScenario: RunScenario,
    private val setDevicePower: SetDevicePower,
    private val setVoiceProfile: SetVoiceProfile,
    private val adjustLight: AdjustLight,
    private val speakOnStation: SpeakOnStation,
    private val preferences: ControlPreferences,
    private val favorites: FavoritePreferences,
    private val alarms: AlarmRepository,
    private val timers: TimerRepository,
    private val reminders: ReminderRepository,
    private val scenarios: ScenarioRepository,
    private val clock: ServerClock,
) : BaseViewModel<ControlUiState>(ControlUiState(configured = loadCatalog.isConfigured)) {
    private var refreshJob: Job? = null
    private var hubJob: Job? = null
    private var generation = 0
    private var hubGeneration = 0
    private var filterChosenByUser = false
    private val expectedPower = HashMap<String, Pair<PowerState, Long>>()

    init {
        // Restore the last selected filter. A tap that happens before this
        // returns wins; an unknown room is reset to All once the catalog loads.
        launch {
            val saved = preferences.loadFilter() ?: return@launch
            if (!filterChosenByUser) updateState { copy(filter = ControlItems.effectiveFilter(entries, saved)) }
        }
        launch {
            val saved = favorites.load() ?: return@launch
            updateState { copy(favorites = saved) }
        }
    }

    /** [silent] refreshes without the pull-to-refresh spinner (used to reconcile after a command). */
    fun refresh(silent: Boolean = false) {
        if (!loadCatalog.isConfigured) {
            updateState { copy(configured = false, refreshing = false, entries = emptyList(), loadError = null) }
            launch { loadHub(catalogOk = false, requested = ++hubGeneration) }
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
            loadHub(catalogOk = result is Outcome.Success, requested = ++hubGeneration)
        }
    }

    /**
     * Loads the live carousel and pinned shortcuts. A failing source simply
     * contributes no cards: the hub is optional decoration over the catalog,
     * so one unreachable feature must not replace the whole Home screen.
     */
    private suspend fun loadHub(catalogOk: Boolean, requested: Int) {
        timers.syncClock()
        val alarmList = alarms.list().getOrNull().orEmpty()
        val timerList = timers.list().getOrNull().orEmpty()
        val scenarioList = scenarios.list().getOrNull().orEmpty()
        val reminderList = reminders.list().getOrNull().orEmpty()
        if (requested != hubGeneration) return
        val now = clock.nowUnix()
        val cards = HubCards.build(alarmList, timerList, scenarioList, reminderList, now)
        val remaining = cards.filterIsInstance<HubCard.TimerCard>()
            .associate { it.key to HubFormat.timerRemaining(it.timer, now) }
        val entries = currentState.entries
        val refs = resolveFavorites(entries, scenarioList, catalogOk)
        updateState {
            copy(
                hubCards = cards,
                hubRemaining = remaining,
                scenarios = scenarioList,
                favorites = refs,
                tick = tick + 1,
                nowUnix = now,
            )
        }
    }

    /** Never persist defaults computed from a catalog that failed to load. */
    private suspend fun resolveFavorites(
        entries: List<ControlEntry>,
        scenarioList: List<Scenario>,
        catalogOk: Boolean,
    ): List<FavoriteRef> {
        favorites.load()?.let { return it }
        if (!catalogOk) return currentState.favorites
        val defaults = Favorites.defaults(entries, scenarioList)
        favorites.save(defaults)
        return defaults
    }

    /** Re-reads only the hub sources (after a card action), keeping the catalog as is. */
    private fun refreshHub() {
        hubJob?.cancel()
        hubJob = launch { loadHub(catalogOk = currentState.entries.isNotEmpty(), requested = ++hubGeneration) }
    }

    /** Countdown tick: recomputes the timer cards' remaining seconds once a second. */
    fun tick() {
        val timers = currentState.hubCards.filterIsInstance<HubCard.TimerCard>()
        if (timers.isEmpty()) return
        val now = clock.nowUnix()
        updateState {
            copy(
                hubRemaining = timers.associate { it.key to HubFormat.timerRemaining(it.timer, now) },
                tick = tick + 1,
                nowUnix = now,
            )
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
     * Queues a spoken text. An invalid or empty text is ignored (the dialog
     * keeps its Say button disabled), and the executor re-validates it again.
     */
    fun speak(entry: ControlEntry, text: String) {
        val message = SpeakText.normalize(text) ?: return
        if (entry.id in currentState.pending || !entry.available) return
        updateState { copy(pending = pending + entry.id, actionError = null) }
        launch {
            when (val result = speakOnStation(entry, message)) {
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
    fun setPower(entry: ControlEntry, on: Boolean) =
        changePower(entry, on) { target, value -> setDevicePower(target, value) }

    /**
     * Switches the assistant profile. It is the profile switch entity's on/off,
     * so it reuses the device path: optimistic update, then re-read until the
     * server agrees. `work = true` sends turn_on (the named profile).
     */
    fun setProfile(entry: ControlEntry, work: Boolean) =
        changePower(entry, work) { target, value -> setVoiceProfile(target, value) }

    private fun changePower(
        entry: ControlEntry,
        on: Boolean,
        send: suspend (ControlEntry, Boolean) -> Outcome<Unit>,
    ) {
        if (!entry.available) return
        val target = if (on) PowerState.ON else PowerState.OFF
        expectedPower[entry.id] = target to SystemClock.elapsedRealtime() + EXPECT_TTL_MS
        updateState {
            copy(actionError = null, entries = entries.map { if (it.id == entry.id) it.copy(power = target) else it })
        }
        launch {
            when (val result = send(entry, on)) {
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

    /** Opens or closes the settings rows of one device card. */
    fun toggleDeviceSettings(deviceId: String) = updateState {
        copy(expandedDevices = if (deviceId in expandedDevices) expandedDevices - deviceId else expandedDevices + deviceId)
    }

    /** Folds or unfolds a room section. */
    fun toggleRoom(section: String) = updateState {
        copy(collapsedRooms = if (section in collapsedRooms) collapsedRooms - section else collapsedRooms + section)
    }

    // --- pinned quick actions ---------------------------------------------

    fun toggleFavorite(entry: ControlEntry) =
        toggleFavoriteRef(FavoriteRef(FavoriteKind.DEVICE, entry.id), entry.displayName)

    fun toggleFavorite(scenario: Scenario) =
        toggleFavoriteRef(FavoriteRef(FavoriteKind.SCENARIO, scenario.id), scenario.label)

    fun toggleFavoriteRef(ref: FavoriteRef, title: String) {
        val updated = Favorites.toggle(currentState.favorites, ref)
        val pinned = ref in updated
        updateState {
            copy(
                favorites = updated,
                notice = ControlNotice.FavoriteChanged(title, pinned),
            )
        }
        launch { favorites.save(updated) }
    }

    // --- live carousel actions --------------------------------------------

    fun hubPrimary(card: HubCard) {
        when (card) {
            is HubCard.Alarm -> if (card.ringing) stopAlarm(card.schedule) else toggleAlarm(card)
            is HubCard.TimerCard -> toggleTimerPause(card.timer)
            is HubCard.ScenarioCard -> runScenarioNow(card.scenario)
            is HubCard.Reminder -> Unit
        }
    }

    fun hubSecondary(card: HubCard) {
        when (card) {
            is HubCard.Alarm -> if (card.ringing) snoozeAlarm(card.schedule) else Unit
            is HubCard.TimerCard -> cancelTimer(card.timer)
            is HubCard.ScenarioCard, is HubCard.Reminder -> Unit
        }
    }

    private fun toggleAlarm(card: HubCard.Alarm) {
        val alarm = card.schedule
        if (alarm.id in currentState.pendingAlarms) return
        updateState { copy(pendingAlarms = pendingAlarms + alarm.id, actionError = null) }
        launch {
            when (val result = alarms.setEnabled(alarm.id, !alarm.enabled)) {
                is Outcome.Success -> {
                    updateState { copy(pendingAlarms = pendingAlarms - alarm.id) }
                    refreshHub()
                }
                is Outcome.Failure -> updateState {
                    copy(pendingAlarms = pendingAlarms - alarm.id, actionError = result.failure)
                }
            }
        }
    }

    private fun stopAlarm(alarm: dev.proflyder.home.core.model.Schedule) {
        if (alarm.id in currentState.pendingAlarms) return
        updateState { copy(pendingAlarms = pendingAlarms + alarm.id, actionError = null) }
        launch {
            when (val result = alarms.stop(alarm.id)) {
                is Outcome.Success -> {
                    updateState { copy(pendingAlarms = pendingAlarms - alarm.id) }
                    refreshHub()
                }
                is Outcome.Failure -> updateState {
                    copy(pendingAlarms = pendingAlarms - alarm.id, actionError = result.failure)
                }
            }
        }
    }

    private fun snoozeAlarm(alarm: dev.proflyder.home.core.model.Schedule) {
        if (alarm.id in currentState.pendingAlarms) return
        updateState { copy(pendingAlarms = pendingAlarms + alarm.id, actionError = null) }
        launch {
            when (val result = alarms.snooze(alarm.id, SNOOZE_MINUTES)) {
                is Outcome.Success -> {
                    updateState { copy(pendingAlarms = pendingAlarms - alarm.id) }
                    refreshHub()
                }
                is Outcome.Failure -> updateState {
                    copy(pendingAlarms = pendingAlarms - alarm.id, actionError = result.failure)
                }
            }
        }
    }

    private fun toggleTimerPause(timer: Timer) {
        if (timer.id in currentState.pendingTimers) return
        updateState { copy(pendingTimers = pendingTimers + timer.id, actionError = null) }
        launch {
            val result = if (timer.state == TimerState.PAUSED) timers.resume(timer.id) else timers.pause(timer.id)
            when (result) {
                is Outcome.Success -> {
                    updateState { copy(pendingTimers = pendingTimers - timer.id) }
                    refreshHub()
                }
                is Outcome.Failure -> updateState {
                    copy(pendingTimers = pendingTimers - timer.id, actionError = result.failure)
                }
            }
        }
    }

    private fun cancelTimer(timer: Timer) {
        if (timer.id in currentState.pendingTimers) return
        updateState { copy(pendingTimers = pendingTimers + timer.id, actionError = null) }
        launch {
            when (val result = timers.cancel(timer.id)) {
                is Outcome.Success -> {
                    updateState { copy(pendingTimers = pendingTimers - timer.id) }
                    refreshHub()
                }
                is Outcome.Failure -> updateState {
                    copy(pendingTimers = pendingTimers - timer.id, actionError = result.failure)
                }
            }
        }
    }

    /** Queues a native scenario run (one tap = one logical run, so one key per tap). */
    fun runScenarioNow(scenario: Scenario) {
        if (scenario.id in currentState.pendingScenarios) return
        updateState { copy(pendingScenarios = pendingScenarios + scenario.id, actionError = null) }
        launch {
            when (val result = scenarios.run(scenario.id, UUID.randomUUID().toString())) {
                is Outcome.Success -> updateState {
                    copy(
                        pendingScenarios = pendingScenarios - scenario.id,
                        notice = ControlNotice.ScenarioQueued(scenario.label),
                    )
                }
                is Outcome.Failure -> updateState {
                    copy(pendingScenarios = pendingScenarios - scenario.id, actionError = result.failure)
                }
            }
        }
    }

    fun consumeNotice() = updateState { copy(notice = null) }

    fun consumeActionError() = updateState { copy(actionError = null) }

    fun consumeLoadError() = updateState { copy(loadError = null) }

    private companion object {
        const val RECONCILE_DELAY_MS = 1_500L
        const val RECONCILE_ATTEMPTS = 8
        const val EXPECT_TTL_MS = 15_000L
        const val SNOOZE_MINUTES = 9
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class ControlViewModelFactoryContributor(
    private val loadCatalog: LoadControlCatalog,
    private val runScenario: RunScenario,
    private val setDevicePower: SetDevicePower,
    private val setVoiceProfile: SetVoiceProfile,
    private val adjustLight: AdjustLight,
    private val speakOnStation: SpeakOnStation,
    private val preferences: ControlPreferences,
    private val favorites: FavoritePreferences,
    private val alarms: AlarmRepository,
    private val timers: TimerRepository,
    private val reminders: ReminderRepository,
    private val scenarios: ScenarioRepository,
    private val clock: ServerClock,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ControlViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        ControlViewModel(
            loadCatalog,
            runScenario,
            setDevicePower,
            setVoiceProfile,
            adjustLight,
            speakOnStation,
            preferences,
            favorites,
            alarms,
            timers,
            reminders,
            scenarios,
            clock,
        )
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

package dev.proflyder.home.feature.control.presentation

import android.os.SystemClock
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.ControlCatalog
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
import dev.proflyder.home.core.model.VolumeControls
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItems
import dev.proflyder.home.feature.control.ControlPreferences
import dev.proflyder.home.feature.call.CallLauncher
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.HubCards
import dev.proflyder.home.feature.control.HubFormat
import dev.proflyder.home.feature.control.SpeakText
import dev.proflyder.home.feature.control.WakeFeedbackAction
import dev.proflyder.home.feature.control.WakeFeedbackKind
import dev.proflyder.home.feature.control.domain.AdjustLight
import dev.proflyder.home.feature.control.domain.LoadControlCatalog
import dev.proflyder.home.feature.control.domain.PressControl
import dev.proflyder.home.feature.control.domain.RunScenario
import dev.proflyder.home.feature.control.domain.SetDevicePower
import dev.proflyder.home.feature.control.domain.SetVoiceProfile
import dev.proflyder.home.feature.control.domain.SetVolume
import dev.proflyder.home.feature.control.domain.SetVolumeMuted
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

sealed interface ControlNotice {
    data class ScenarioQueued(val title: String) : ControlNotice

    data class FavoriteChanged(val title: String, val pinned: Boolean) : ControlNotice

    data class WakeMarked(val kind: WakeFeedbackKind) : ControlNotice

    data object CommandSent : ControlNotice
}

data class ControlUiState(
    val configured: Boolean = true,
    val refreshing: Boolean = true,
    val entries: List<ControlEntry> = emptyList(),
    val statesOk: Boolean = true,
    val pending: Set<String> = emptySet(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    val notice: ControlNotice? = null,
    val filter: ControlFilter = ControlFilter.All,
    val expandedDevices: Set<String> = emptySet(),
    val collapsedRooms: Set<String> = emptySet(),
    val hubCards: List<HubCard> = emptyList(),
    val hubRemaining: Map<String, Long> = emptyMap(),
    val scenarios: List<Scenario> = emptyList(),
    val favorites: List<FavoriteRef> = emptyList(),
    val pendingScenarios: Set<String> = emptySet(),
    val pendingTimers: Set<String> = emptySet(),
    val pendingAlarms: Set<String> = emptySet(),
    val tick: Long = 0,
    val nowUnix: Long = 0,
)

class ControlViewModel(
    private val loadCatalog: LoadControlCatalog,
    private val runScenario: RunScenario,
    private val pressControl: PressControl,
    private val setDevicePower: SetDevicePower,
    private val setVoiceProfile: SetVoiceProfile,
    private val adjustLight: AdjustLight,
    private val speakOnStation: SpeakOnStation,
    private val setStationVolume: SetVolume,
    private val setStationMuted: SetVolumeMuted,
    private val preferences: ControlPreferences,
    private val favorites: FavoritePreferences,
    private val alarms: AlarmRepository,
    private val timers: TimerRepository,
    private val reminders: ReminderRepository,
    private val scenarios: ScenarioRepository,
    private val clock: ServerClock,
    private val callLauncher: CallLauncher,
) : BaseViewModel<ControlUiState>(ControlUiState(configured = loadCatalog.isConfigured)) {
    private var refreshJob: Job? = null
    private var hubJob: Job? = null
    private var generation = 0
    private var hubGeneration = 0
    private var filterChosenByUser = false
    private val expectedPower = HashMap<String, Pair<PowerState, Long>>()
    private val expectedVolume = HashMap<String, Pair<VolumeControls, Long>>()

    init {
        launch {
            val saved = preferences.loadFilter() ?: return@launch
            if (!filterChosenByUser) updateState { copy(filter = ControlItems.effectiveFilter(entries, saved)) }
        }
        launch {
            val saved = favorites.load() ?: return@launch
            updateState { copy(favorites = saved) }
        }
    }

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
            applyRefreshResult(result)
        }
    }

    private suspend fun applyRefreshResult(result: Outcome<ControlCatalog>) {
        when (result) {
            is Outcome.Success -> updateState {
                copy(
                    refreshing = false,
                    entries = withExpectations(result.value.entries),
                    statesOk = result.value.statesOk,
                    filter = ControlItems.effectiveFilter(result.value.entries, filter),
                )
            }
            is Outcome.Failure -> updateState { copy(refreshing = false, loadError = result.failure) }
        }
        loadHub(catalogOk = result is Outcome.Success, requested = ++hubGeneration)
    }

    private suspend fun loadHub(catalogOk: Boolean, requested: Int) {
        timers.syncClock()
        val alarmsResult = alarms.list()
        val timersResult = timers.list()
        val scenariosResult = scenarios.list()
        val remindersResult = reminders.list()
        if (requested != hubGeneration) return
        val now = clock.nowUnix()
        val cards = HubCards.buildOrKeep(
            alarms = alarmsResult.getOrNull(),
            timers = timersResult.getOrNull(),
            reminders = remindersResult.getOrNull(),
            previous = currentState.hubCards,
            nowUnix = now,
        )
        val remaining = cards.filterIsInstance<HubCard.TimerCard>()
            .associate { it.key to HubFormat.timerRemaining(it.timer, now) }
        val entries = currentState.entries
        val allScenarios = scenariosResult.getOrNull() ?: currentState.scenarios
        val refs = resolveFavorites(entries, allScenarios, catalogOk)
        updateState {
            copy(
                hubCards = cards,
                hubRemaining = remaining,
                scenarios = allScenarios,
                favorites = refs,
                tick = tick + 1,
                nowUnix = now,
            )
        }
    }

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

    private fun refreshHub() {
        hubJob?.cancel()
        hubJob = launch { loadHub(catalogOk = currentState.entries.isNotEmpty(), requested = ++hubGeneration) }
    }

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

    fun markWake(action: WakeFeedbackAction) {
        val entry = action.entry
        if (action.busy || !entry.available || entry.id in currentState.pending) return
        updateState { copy(pending = pending + entry.id, actionError = null) }
        launch {
            when (val result = runScenario(entry)) {
                is Outcome.Success -> updateState {
                    copy(pending = pending - entry.id, notice = ControlNotice.WakeMarked(action.kind))
                }
                is Outcome.Failure -> updateState { copy(pending = pending - entry.id, actionError = result.failure) }
            }
        }
    }

    fun press(entry: ControlEntry) {
        if (entry.id in currentState.pending || !entry.available) return
        updateState { copy(pending = pending + entry.id, actionError = null) }
        launch {
            when (val result = pressControl(entry)) {
                is Outcome.Success -> {
                    updateState {
                        copy(pending = pending - entry.id, notice = ControlNotice.CommandSent)
                    }
                    refresh(silent = true)
                    refreshAfterPress()
                }
                is Outcome.Failure -> updateState {
                    copy(pending = pending - entry.id, actionError = result.failure)
                }
            }
        }
    }

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

    fun setPower(entry: ControlEntry, on: Boolean) =
        changePower(entry, on) { target, value -> setDevicePower(target, value) }

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

    private suspend fun reconcile(id: String) {
        repeat(RECONCILE_ATTEMPTS) {
            if (!reconcileStep(id)) return
        }
    }

    private suspend fun reconcileStep(id: String): Boolean {
        delay(RECONCILE_DELAY_MS)
        if (expectedPower[id] == null) return false
        refresh(silent = true)
        refreshJob?.join()
        return true
    }

    private fun withExpectations(entries: List<ControlEntry>): List<ControlEntry> {
        if (expectedPower.isEmpty() && expectedVolume.isEmpty()) return entries
        val now = SystemClock.elapsedRealtime()
        return entries.map { applyExpectations(it, now) }
    }

    private fun applyExpectations(entry: ControlEntry, now: Long): ControlEntry {
        val volume = expectedVolume[entry.id]
        if (volume != null) return withVolumeExpectation(entry, volume, now)
        val power = expectedPower[entry.id] ?: return entry
        val (target, deadline) = power
        return when {
            entry.power == target || now >= deadline -> entry.also { expectedPower.remove(entry.id) }
            else -> entry.copy(power = target)
        }
    }

    private fun withVolumeExpectation(
        entry: ControlEntry,
        expectation: Pair<VolumeControls, Long>,
        now: Long,
    ): ControlEntry {
        val (target, deadline) = expectation
        return when {
            entry.volume == target || now >= deadline -> entry.also { expectedVolume.remove(entry.id) }
            else -> entry.copy(volume = target)
        }
    }

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

    fun setVolume(entry: ControlEntry, level: Int) {
        val current = entry.volume ?: return
        if (!entry.available) return
        optimisticVolume(entry, current.copy(level = level))
        launch {
            when (val result = setStationVolume(entry, level)) {
                is Outcome.Success -> reconcileVolume(entry.id)
                is Outcome.Failure -> failVolume(entry.id, result.failure)
            }
        }
    }

    fun setVolumeMuted(entry: ControlEntry, muted: Boolean) {
        val current = entry.volume ?: return
        if (!entry.available) return
        optimisticVolume(entry, current.copy(muted = muted))
        launch {
            when (val result = setStationMuted(entry, muted)) {
                is Outcome.Success -> reconcileVolume(entry.id)
                is Outcome.Failure -> failVolume(entry.id, result.failure)
            }
        }
    }

    private fun optimisticVolume(entry: ControlEntry, target: VolumeControls) {
        expectedVolume[entry.id] = target to SystemClock.elapsedRealtime() + EXPECT_TTL_MS
        updateState {
            copy(
                actionError = null,
                entries = entries.map { if (it.id == entry.id) it.copy(volume = target) else it },
            )
        }
    }

    private fun failVolume(id: String, failure: RemoteFailure) {
        expectedVolume.remove(id)
        updateState { copy(actionError = failure) }
        refresh(silent = true)
    }

    private suspend fun reconcileVolume(id: String) {
        delay(VOLUME_SETTLE_DELAY_MS)
        if (expectedVolume[id] == null) return
        refresh(silent = true)
        refreshJob?.join()
    }

    fun setFilter(filter: ControlFilter) {
        filterChosenByUser = true
        updateState { copy(filter = filter) }
        launch { preferences.saveFilter(filter) }
    }

    fun toggleDeviceSettings(deviceId: String) = updateState {
        copy(expandedDevices = if (deviceId in expandedDevices) expandedDevices - deviceId else expandedDevices + deviceId)
    }

    fun toggleRoom(section: String) = updateState {
        copy(collapsedRooms = if (section in collapsedRooms) collapsedRooms - section else collapsedRooms + section)
    }

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

    fun hubPrimary(card: HubCard) {
        when (card) {
            is HubCard.Alarm -> if (card.ringing) stopAlarm(card.schedule) else toggleAlarm(card)
            is HubCard.TimerCard -> toggleTimerPause(card.timer)
            is HubCard.Reminder -> Unit
        }
    }

    private fun toggleAlarm(card: HubCard.Alarm) = changeAlarmEnabled(card, !card.schedule.enabled)

    fun disableAlarm(card: HubCard.Alarm) = changeAlarmEnabled(card, false)

    fun skipAlarmOccurrence(card: HubCard.Alarm) {
        val alarm = card.schedule
        if (alarm.id in currentState.pendingAlarms) return
        updateState { copy(pendingAlarms = pendingAlarms + alarm.id, actionError = null) }
        launch {
            when (val result = alarms.skipNextOccurrence(alarm.id)) {
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

    private fun changeAlarmEnabled(card: HubCard.Alarm, enabled: Boolean) {
        val alarm = card.schedule
        if (alarm.id in currentState.pendingAlarms) return
        updateState { copy(pendingAlarms = pendingAlarms + alarm.id, actionError = null) }
        launch {
            when (val result = alarms.setEnabled(alarm.id, enabled)) {
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

    fun callStation() = callLauncher.openCallScreen()

    fun consumeNotice() = updateState { copy(notice = null) }

    fun consumeActionError() = updateState { copy(actionError = null) }

    fun consumeLoadError() = updateState { copy(loadError = null) }

    private suspend fun refreshAfterPress() {
        delay(PRESS_SETTLE_DELAY_MS)
        refresh(silent = true)
    }

    private companion object {
        const val PRESS_SETTLE_DELAY_MS = 4_500L
        const val VOLUME_SETTLE_DELAY_MS = 1_500L
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
    private val pressControl: PressControl,
    private val setDevicePower: SetDevicePower,
    private val setVoiceProfile: SetVoiceProfile,
    private val adjustLight: AdjustLight,
    private val speakOnStation: SpeakOnStation,
    private val setStationVolume: SetVolume,
    private val setStationMuted: SetVolumeMuted,
    private val preferences: ControlPreferences,
    private val favorites: FavoritePreferences,
    private val alarms: AlarmRepository,
    private val timers: TimerRepository,
    private val reminders: ReminderRepository,
    private val scenarios: ScenarioRepository,
    private val clock: ServerClock,
    private val callLauncher: CallLauncher,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ControlViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        ControlViewModel(
            loadCatalog,
            runScenario,
            pressControl,
            setDevicePower,
            setVoiceProfile,
            adjustLight,
            speakOnStation,
            setStationVolume,
            setStationMuted,
            preferences,
            favorites,
            alarms,
            timers,
            reminders,
            scenarios,
            clock,
            callLauncher,
        )
    }
}

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

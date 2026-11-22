package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.navigation.Route
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.model.ServerClockSync
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.control.ControlPreferences
import dev.proflyder.home.feature.control.ControlRepository
import dev.proflyder.home.feature.control.QuickTileProvider
import dev.proflyder.home.feature.control.data.HomeHubFeed
import dev.proflyder.home.feature.control.data.HubSources
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.proflyder.home.feature.timers.TimerRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal class ControlScreenDependencies(
    val repository: ControlRepository,
    val preferences: ControlPreferences,
    val favorites: FavoritePreferences,
    val providers: Set<QuickTileProvider>,
    val hub: HubSources,
    val clock: ServerClock,
)

internal class ControlViewModel(
    dependencies: ControlScreenDependencies,
) : StateViewModel<ControlUiState, ControlIntent, ControlEvent>(ControlUiState()) {
    private val hubFeed = HomeHubFeed(dependencies.hub, viewModelScope)
    private val tiles = QuickTileLauncher(dependencies.providers)
    private val favorites = ControlFavorites(dependencies.favorites)
    private val devices = DeviceCommands(dependencies.repository, dependencies.hub.scenarios)
    private val hub = HubCommands(dependencies.hub.alarms, dependencies.hub.timers, hubFeed)
    private val preferences = dependencies.preferences
    private val layout = ControlLayoutState()
    private val sources = ControlScreenSources(dependencies, hubFeed, favorites, tiles)

    init {
        publish(currentState.copy(provided = tiles.tiles))
        launch { sources.state(devices.pendingEntries, devices.pendingScenarios, layout.state).collect(::publish) }
        launch { sources.loadFailures().collect { emit(ControlEvent.Failure(it)) } }
    }

    override fun handle(intent: ControlIntent) {
        when (intent) {
            ControlIntent.Refresh -> sources.refresh()
            is ControlIntent.Tap -> tap(intent.target)
            is ControlIntent.Hold -> react(ControlTaps.hold(intent.target, currentState))
            is ControlIntent.Command -> report { devices.perform(intent.command) }
            is ControlIntent.Hub -> report { hub.perform(intent.command) }
            is ControlIntent.AskPin -> emit(ControlEvent.ConfirmPin(intent.entry, intent.entry.devicePin in currentState.favorites))
            is ControlIntent.TogglePin -> report { favorites.toggle(intent.subject) }
            is ControlIntent.SelectFilter -> report { saveFilter(intent) }
            is ControlIntent.ToggleDevice -> layout.toggleDevice(intent.deviceId)
            is ControlIntent.ToggleRoom -> layout.toggleRoom(intent.section)
            is ControlIntent.Dial -> layout.setDial(intent.open)
            is ControlIntent.ActivateTile -> tiles.activate(intent.key)?.let(::emit)
            ControlIntent.OpenConnection -> emit(ControlEvent.OpenRoute(Route.Tab(CONNECTION_FEATURE)))
        }
    }

    private fun publish(fresh: ControlUiState) = updateState { fresh }

    private fun tap(target: TapTarget) {
        if (target is TapTarget.Dial) layout.setDial(false)
        react(ControlTaps.tap(target, currentState))
    }

    private fun react(reaction: ControlReaction) {
        when (reaction) {
            is ControlReaction.Dispatch -> handle(reaction.intent)
            is ControlReaction.Emit -> emit(reaction.event)
            ControlReaction.None -> Unit
        }
    }

    private fun report(block: suspend () -> ControlEvent?) {
        launch { block()?.let(::emit) }
    }

    private suspend fun saveFilter(intent: ControlIntent.SelectFilter): ControlEvent? {
        preferences.saveFilter(intent.filter)
        return null
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(ControlViewModel::class)
@Inject
class ControlViewModelSource internal constructor(
    repository: ControlRepository,
    preferences: ControlPreferences,
    favorites: FavoritePreferences,
    providers: Set<QuickTileProvider>,
    alarms: AlarmRepository,
    timers: TimerRepository,
    reminders: ReminderRepository,
    scenarios: ScenarioRepository,
    clock: ServerClock,
    clockSync: ServerClockSync,
    foreground: AppForeground,
) : ViewModelSource {
    private val dependencies = ControlScreenDependencies(
        repository = repository,
        preferences = preferences,
        favorites = favorites,
        providers = providers,
        hub = HubSources(alarms, timers, reminders, scenarios, clockSync, foreground),
        clock = clock,
    )

    override fun create(): ViewModel = ControlViewModel(dependencies)
}

private const val CONNECTION_FEATURE = "connection"

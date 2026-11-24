package dev.proflyder.home.feature.control.presentation

import android.content.Context
import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.navigation.Route
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.control.ControlPreferences
import dev.proflyder.home.feature.control.ControlRepository
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.QuickTileProvider
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.Lease
import dev.proflyder.home.feature.control.di.ControlGraph
import dev.proflyder.home.core.presentation.holding
import dev.proflyder.home.feature.control.di.ControlFeatureScope
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.proflyder.home.feature.timers.TimerRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal class HubSources(
    val timers: TimerRepository,
    val reminders: ReminderRepository,
    val scenarios: ScenarioRepository,
)

internal class ControlScreenDependencies(
    val repository: ControlRepository,
    val preferences: ControlPreferences,
    val favorites: FavoritePreferences,
    val defaultFavoriteScenarios: Set<String>,
    val providers: Set<QuickTileProvider>,
    val hub: HubSources,
    val clock: ServerClock,
    val access: AppAccessSource,
)

internal class ControlViewModel(
    dependencies: ControlScreenDependencies,
) : StateViewModel<ControlUiState, ControlIntent, ControlEvent>(ControlUiState()) {
    override fun derive(previous: ControlUiState, next: ControlUiState): ControlUiState = next.derivedFrom(previous)

    private val tiles = QuickTileLauncher(dependencies.providers, dependencies.access)
    private val favorites = ControlFavorites(dependencies.favorites, dependencies.defaultFavoriteScenarios)
    private val devices = DeviceCommands(dependencies.repository, dependencies.hub.scenarios)
    private val hub = HubCommands(dependencies.hub.timers)
    private val preferences = dependencies.preferences
    private val layout = ControlLayoutState()
    private val sources = ControlScreenSources(dependencies, favorites, tiles)

    init {
        publish(currentState.copy(provided = tiles.tiles))
        launch { dependencies.access.state.collect { updateState { copy(provided = tiles.tiles) } } }
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
    private val context: Context,
    private val control: ControlFeatureScope,
    private val preferences: ControlPreferences,
    private val favorites: FavoritePreferences,
    private val providers: Set<QuickTileProvider>,
    private val timers: FeatureScope<TimerRepository>,
    private val reminders: FeatureScope<ReminderRepository>,
    private val scenarios: FeatureScope<ScenarioRepository>,
    private val clock: ServerClock,
    private val access: AppAccessSource,
) : ViewModelSource {
    override fun create(): ViewModel {
        val leases = ControlLeases(control.acquire(), timers.acquire(), reminders.acquire(), scenarios.acquire())
        return ControlViewModel(dependencies(leases)).holding(leases.all)
    }

    private fun dependencies(leases: ControlLeases) = ControlScreenDependencies(
        repository = leases.control.graph.repository,
        preferences = preferences,
        favorites = favorites,
        defaultFavoriteScenarios = context.resources.getStringArray(R.array.control_default_favorite_scenarios).toSet(),
        providers = providers,
        hub = HubSources(leases.timers.graph, leases.reminders.graph, leases.scenarios.graph),
        clock = clock,
        access = access,
    )
}

private class ControlLeases(
    val control: Lease<ControlGraph>,
    val timers: Lease<TimerRepository>,
    val reminders: Lease<ReminderRepository>,
    val scenarios: Lease<ScenarioRepository>,
) {
    val all: List<Lease<*>> = listOf(control, timers, reminders, scenarios)
}

private const val CONNECTION_FEATURE = "connection"

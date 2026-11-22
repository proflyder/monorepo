package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.runtime.tickerFlow
import dev.proflyder.home.feature.control.ControlCatalogState
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.HubCards
import dev.proflyder.home.feature.control.HubFormat
import dev.proflyder.home.feature.control.data.HomeHubFeed
import dev.proflyder.home.feature.control.data.HubData
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

internal class ControlScreenSources(
    dependencies: ControlScreenDependencies,
    private val hubFeed: HomeHubFeed,
    private val favorites: ControlFavorites,
    private val tiles: QuickTileLauncher,
) {
    private val repository = dependencies.repository
    private val preferences = dependencies.preferences
    private val clock = dependencies.clock

    fun state(
        pending: Flow<Set<String>>,
        pendingScenarios: Flow<Set<String>>,
        layout: Flow<ControlLayout>,
    ): Flow<ControlUiState> = combine(remote(), pending, pendingScenarios, layout) { base, entries, scenarios, view ->
        base.copy(pending = entries, pendingScenarios = scenarios, layout = view)
    }

    fun loadFailures(): Flow<RemoteFailure> = repository.catalog
        .map { state -> state.failure?.takeIf { state.entries.isNotEmpty() } }
        .distinctUntilChanged()
        .filterNotNull()

    fun refresh() {
        repository.refresh()
        hubFeed.refresh()
    }

    private fun remote(): Flow<ControlUiState> {
        val scenarios = hubFeed.state.map { it.scenarios }.distinctUntilChanged()
        return combine(
            repository.catalog,
            hubView(),
            scenarios,
            favorites.resolved(repository.catalog, scenarios),
            preferences.filter.map { it ?: ControlFilter.All },
            ::assemble,
        )
    }

    private fun assemble(
        catalog: ControlCatalogState,
        hub: HubView,
        scenarios: List<Scenario>?,
        pinned: List<FavoriteRef>,
        filter: ControlFilter,
    ): ControlUiState = ControlUiState(
        catalog = catalog,
        hub = hub,
        scenarios = scenarios.orEmpty(),
        favorites = pinned,
        provided = tiles.tiles,
        filter = filter,
    )

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun hubView(): Flow<HubView> = hubFeed.state.flatMapLatest { data ->
        nowUnix(data).map { now -> data.toView(now) }
    }

    private fun nowUnix(data: HubData): Flow<Long> = when (data.timers.orEmpty().any { it.state == TimerState.RUNNING }) {
        true -> tickerFlow(TICK).map { clock.nowUnix() }
        false -> flowOf(clock.nowUnix())
    }

    private fun HubData.toView(now: Long): HubView {
        val cards = HubCards.build(alarms.orEmpty(), timers.orEmpty(), reminders.orEmpty(), now)
        val remaining = cards.filterIsInstance<HubCard.TimerCard>()
            .associate { it.key to HubFormat.timerRemaining(it.timer, now) }
        return HubView(cards, remaining, now)
    }

    private companion object {
        val TICK = 1.seconds
    }
}

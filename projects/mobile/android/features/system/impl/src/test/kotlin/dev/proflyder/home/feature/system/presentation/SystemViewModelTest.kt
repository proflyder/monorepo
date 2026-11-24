package dev.proflyder.home.feature.system.presentation

import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.feature.system.SystemDetailDestination
import dev.proflyder.home.feature.system.SystemFixtures
import dev.proflyder.home.feature.system.SystemRepository
import dev.proflyder.home.feature.system.SystemState
import dev.proflyder.home.feature.system.data.PollingSystemRepository
import dev.proflyder.home.feature.system.data.SystemRefreshPolicy
import dev.proflyder.home.feature.system.data.combine
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SystemViewModelTest {
    private class FakeRepository : SystemRepository {
        val mutable = MutableStateFlow(SystemState())
        override val state: StateFlow<SystemState> = mutable
        var refreshes = 0

        override fun refresh() {
            refreshes++
        }
    }

    private class FakeAccess : AppAccessSource {
        val mutable = MutableStateFlow<AppAccessState>(AppAccessState.Unknown)
        override val state: StateFlow<AppAccessState> = mutable
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun theScreenMirrorsTheRepositoryAndReportsAFailedRefreshOnce() = runTest(UnconfinedTestDispatcher()) {
        val repository = FakeRepository()
        val access = FakeAccess()
        val viewModel = SystemViewModel(repository, access)
        val events = mutableListOf<SystemEvent>()
        backgroundScope.launch { viewModel.events.toList(events) }

        repository.mutable.value = SystemState(loading = false, snapshot = SystemFixtures.snapshot)
        assertEquals(SystemFixtures.snapshot, viewModel.state.value.snapshot.snapshot)

        repository.mutable.value = repository.mutable.value.copy(loading = true)
        repository.mutable.value = SystemState(false, SystemFixtures.snapshot, RemoteFailure.Offline("down"))
        assertEquals(listOf(SystemEvent.RefreshFailed(RemoteFailure.Offline("down"))), events)

        access.mutable.value = AppAccessState.Ready(
            AppAccess("Owner", AppModule.entries.toSet(), owner = true),
        )
        assertTrue(viewModel.state.value.administrator)

        viewModel.dispatch(SystemIntent.Refresh)
        viewModel.dispatch(SystemIntent.OpenSection(SystemSection.CLUSTER))
        assertEquals(1, repository.refreshes)
        assertEquals(SystemEvent.OpenRoute(Route.Screen(SystemDetailDestination(SystemSection.CLUSTER))), events.last())
    }

    @Test
    fun theRepositoryPollsOnlyWhileObservedAndKeepsTheLastSnapshot() = runTest(StandardTestDispatcher()) {
        val results = ArrayDeque<Outcome<SystemSnapshot>>(
            listOf(Outcome.Success(SystemFixtures.snapshot), Outcome.Failure(RemoteFailure.Offline("down"))),
        )
        var calls = 0
        val repository = PollingSystemRepository(
            load = {
                calls++
                results.removeFirstOrNull() ?: Outcome.Success(SystemFixtures.empty)
            },
            scope = backgroundScope,
            policy = SystemRefreshPolicy(period = 30.seconds, stopTimeout = 1.seconds),
        )
        runCurrent()
        assertEquals(0, calls)

        val watcher = backgroundScope.launch { repository.state.collect {} }
        runCurrent()
        assertEquals(1, calls)
        assertEquals(SystemFixtures.snapshot, repository.state.value.snapshot)
        assertFalse(repository.state.value.loading)

        repository.refresh()
        runCurrent()
        assertEquals(2, calls)
        assertEquals(SystemFixtures.snapshot, repository.state.value.snapshot)
        assertEquals(RemoteFailure.Offline("down"), repository.state.value.error)

        advanceTimeBy(30.seconds)
        runCurrent()
        assertEquals(3, calls)
        assertNull(repository.state.value.error)

        watcher.cancel()
        advanceTimeBy(120.seconds)
        runCurrent()
        assertEquals(3, calls)
        assertTrue(repository.state.value.snapshot != null)
    }

    @Test
    fun combineReturnsTheFirstFailure() {
        val snapshot = SystemFixtures.snapshot
        val ok = combine(
            Outcome.Success(snapshot.summary),
            Outcome.Success(snapshot.hosts),
            Outcome.Success(snapshot.cluster),
            Outcome.Success(snapshot.devices),
        )
        assertEquals(Outcome.Success(snapshot), ok)
        val failed = combine(
            Outcome.Success(snapshot.summary),
            Outcome.Failure(RemoteFailure.Offline("down")),
            Outcome.Success(snapshot.cluster),
            Outcome.Success(snapshot.devices),
        )
        assertTrue(failed is Outcome.Failure)
    }
}

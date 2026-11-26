package dev.proflyder.home.feature.hermes

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.network.HermesPanelClient
import dev.proflyder.home.feature.hermes.data.HttpHermesRepository
import dev.proflyder.home.feature.hermes.presentation.HermesEvent
import dev.proflyder.home.feature.hermes.presentation.HermesIntent
import dev.proflyder.home.feature.hermes.presentation.HermesSection
import dev.proflyder.home.feature.hermes.presentation.HermesViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class HermesViewModelTest {
    private class FakeRepository : HermesRepository {
        val mutable = MutableStateFlow(HermesState())
        override val state: StateFlow<HermesState> = mutable
        val refreshed = mutableListOf<HermesPanel>()

        override fun refresh(panel: HermesPanel) {
            refreshed += panel
        }
    }

    private class FakeClient(private val voice: Outcome<dev.proflyder.home.core.model.VoiceStatus>) : HermesPanelClient {
        override suspend fun agents() = Outcome.Success(HermesFixtures.agents)
        override suspend fun reports() = Outcome.Success(HermesFixtures.reports)
        override suspend fun voice() = voice
        override suspend fun skills() = Outcome.Success(HermesFixtures.skills)
        override suspend fun toolsets() = Outcome.Success(HermesFixtures.toolsets)
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun theScreenMirrorsTheRepositoryAndForwardsRefreshes() = runTest(UnconfinedTestDispatcher()) {
        val repository = FakeRepository()
        val viewModel = HermesViewModel(repository)
        viewModel.dispatch(HermesIntent.Refresh(HermesPanel.VOICE))
        assertEquals(listOf(HermesPanel.VOICE), repository.refreshed)

        repository.mutable.value = HermesFixtures.state
        assertEquals(HermesFixtures.voice, viewModel.state.value.hermes.voice.value)
    }

    @Test
    fun aFailedRefreshWithDataIsReportedOnceAndAFirstFailureIsNot() = runTest(UnconfinedTestDispatcher()) {
        val repository = FakeRepository()
        val viewModel = HermesViewModel(repository)
        val events = mutableListOf<HermesEvent>()
        backgroundScope.launch { viewModel.events.toList(events) }
        val down = RemoteFailure.Offline("down")

        repository.mutable.value = HermesState(agents = Loaded(loading = true))
        repository.mutable.value = HermesState(agents = Loaded(failure = down))
        assertEquals(emptyList<HermesEvent>(), events)

        repository.mutable.value = HermesState(agents = Loaded(loading = true, value = HermesFixtures.agents))
        repository.mutable.value = HermesState(agents = Loaded(value = HermesFixtures.agents, failure = down))
        assertEquals(listOf(HermesEvent.RefreshFailed(down)), events)
    }

    @Test
    fun openingARowEmitsAScreenRoute() = runTest(UnconfinedTestDispatcher()) {
        val viewModel = HermesViewModel(FakeRepository())
        val events = mutableListOf<HermesEvent>()
        backgroundScope.launch { viewModel.events.toList(events) }
        val destination = HermesSectionDestination(HermesSection.REPORTS)
        viewModel.dispatch(HermesIntent.Open(destination))
        assertEquals(listOf(HermesEvent.OpenRoute(Route.Screen(destination))), events)
    }

    @Test
    fun theRepositoryKeepsTheLastValueWhenARefreshFails() = runTest(UnconfinedTestDispatcher()) {
        val repository = HttpHermesRepository(FakeClient(Outcome.Success(HermesFixtures.voice)), backgroundScope)
        repository.refresh(HermesPanel.VOICE)
        assertEquals(HermesFixtures.voice, repository.state.value.voice.value)
        assertFalse(repository.state.value.voice.loading)

        val failing = HttpHermesRepository(FakeClient(Outcome.Failure(RemoteFailure.Offline("down"))), backgroundScope)
        failing.refresh(HermesPanel.VOICE)
        assertNull(failing.state.value.voice.value)
        assertEquals(RemoteFailure.Offline("down"), failing.state.value.voice.failure)
        failing.refresh(HermesPanel.AGENTS)
        assertEquals(HermesFixtures.agents, failing.state.value.agents.value)
    }

    @Test
    fun theCatalogPanelsLoadIndependently() = runTest(UnconfinedTestDispatcher()) {
        val repository = HttpHermesRepository(FakeClient(Outcome.Success(HermesFixtures.voice)), backgroundScope)
        repository.refresh(HermesPanel.TOOLSETS)
        assertEquals(HermesFixtures.toolsets, repository.state.value.toolsets.value)
        assertNull(repository.state.value.skills.value)
        repository.refresh(HermesPanel.SKILLS)
        assertEquals(HermesFixtures.skills, repository.state.value.skills.value)
        assertFalse(repository.state.value.skills.loading)
    }
}

package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.presentation.SharedFlowAppEventBus
import dev.proflyder.home.feature.scenarios.domain.LoadScenarios
import dev.proflyder.home.feature.scenarios.domain.RunScenario
import dev.proflyder.home.feature.scenarios.domain.SetScenarioEnabled
import dev.proflyder.home.feature.scenarios.presentation.ScenariosChanged
import dev.proflyder.home.feature.scenarios.presentation.ScenariosIntent
import dev.proflyder.home.feature.scenarios.presentation.ScenariosViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ScenariosViewModelTest {
    @After
    fun resetMainDispatcher() = Dispatchers.resetMain()

    @Test
    fun editorChangeRefreshesTheListWithoutTheScreenAsking() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val repository = CountingRepository()
        val bus = SharedFlowAppEventBus()
        val viewModel = ScenariosViewModel(
            LoadScenarios(repository),
            SetScenarioEnabled(repository),
            RunScenario(repository),
            NoFavorites,
            bus,
        )
        advanceUntilIdle()
        assertEquals(1, repository.lists)

        bus.publish(ScenariosChanged)
        advanceUntilIdle()
        assertEquals(2, repository.lists)
        assertEquals(listOf("sc-2"), viewModel.state.value.scenarios.map { it.id })
    }

    @Test
    fun toggleRendersOnlyTheServerConfirmedState() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val repository = CountingRepository()
        val viewModel = ScenariosViewModel(
            LoadScenarios(repository),
            SetScenarioEnabled(repository),
            RunScenario(repository),
            NoFavorites,
            SharedFlowAppEventBus(),
        )
        advanceUntilIdle()
        val scenario = viewModel.state.value.scenarios.single()

        viewModel.dispatch(ScenariosIntent.SetEnabled(scenario, enabled = false))
        assertEquals(true, viewModel.state.value.scenarios.single().enabled)
        advanceUntilIdle()
        assertEquals(false, viewModel.state.value.scenarios.single().enabled)
    }

    private object NoFavorites : FavoritePreferences {
        override val favorites: Flow<List<FavoriteRef>?> = flowOf(null)
        override suspend fun saveIfAbsent(refs: List<FavoriteRef>) = Unit
        override suspend fun toggle(ref: FavoriteRef): Boolean = false
    }

    private class CountingRepository : ScenarioRepository {
        var lists = 0

        override suspend fun list(): Outcome<List<Scenario>> = Outcome.Success(listOf(scenario("sc-${++lists}")))
        override suspend fun get(id: String): Outcome<Scenario> = Outcome.Success(scenario(id))
        override suspend fun create(draft: ScenarioDraft, idempotencyKey: String) = Outcome.Success(scenario("new"))
        override suspend fun update(existing: Scenario, draft: ScenarioDraft) = Outcome.Success(existing)
        override suspend fun setEnabled(id: String, enabled: Boolean) = Outcome.Success(scenario(id).copy(enabled = enabled))
        override suspend fun delete(id: String): Outcome<Boolean> = Outcome.Success(true)
        override suspend fun run(id: String, idempotencyKey: String): Outcome<String> = Outcome.Success("evt")
    }

    private companion object {
        fun scenario(id: String) = Scenario(
            id = id,
            label = id,
            description = "",
            enabled = true,
            revision = 1,
            createdUnix = 0,
            updatedUnix = 0,
            triggers = emptyList(),
            conditions = emptyList(),
            steps = emptyList(),
            continueOnError = false,
            nextDueUnix = 0,
        )
    }
}

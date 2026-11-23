package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.feature.scenarios.data.ScenarioRemote
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.feature.scenarios.domain.RunScenario
import dev.proflyder.home.feature.scenarios.domain.SetScenarioEnabled
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
    fun aChangeInTheStoreReachesTheListWithoutAReload() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val remote = CountingRemote()
        val repository = remote.store(Dispatchers.Main)
        val viewModel = viewModel(repository)
        advanceUntilIdle()
        assertEquals(1, remote.lists)

        repository.create(ScenarioDraft(label = "new"), "key")
        advanceUntilIdle()
        assertEquals(1, remote.lists)
        assertEquals(listOf("new", "sc-1"), viewModel.state.value.scenarios.map { it.id })
    }

    @Test
    fun aToggleSettlesInTheSharedStore() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val repository = CountingRemote().store(Dispatchers.Main)
        val viewModel = viewModel(repository)
        advanceUntilIdle()
        val scenario = viewModel.state.value.scenarios.single()

        viewModel.dispatch(ScenariosIntent.SetEnabled(scenario, enabled = false))
        advanceUntilIdle()
        assertEquals(false, viewModel.state.value.scenarios.single().enabled)
        assertEquals(false, repository["sc-1"]?.enabled)
    }

    private fun viewModel(repository: ScenarioRepository) = ScenariosViewModel(
        repository,
        SetScenarioEnabled(repository),
        RunScenario(repository),
        NoFavorites,
    )

    private object NoFavorites : FavoritePreferences {
        override val favorites: Flow<List<FavoriteRef>?> = flowOf(null)
        override suspend fun saveIfAbsent(refs: List<FavoriteRef>) = Unit
        override suspend fun toggle(ref: FavoriteRef): Boolean = false
    }

    private class CountingRemote : ScenarioRemote {
        var lists = 0

        override suspend fun list(): Outcome<List<Scenario>> = Outcome.Success(listOf(scenario("sc-1"))).also { lists++ }
        override suspend fun get(id: String): Outcome<Scenario> = Outcome.Success(scenario(id))
        override suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario> = Outcome.Success(scenario("new"))
        override suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario> = Outcome.Success(existing)
        override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario> =
            Outcome.Success(scenario(id).copy(enabled = enabled))
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

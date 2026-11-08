package dev.proflyder.home.feature.scenarios

import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.network.ControlCatalogClient
import dev.proflyder.home.feature.scenarios.domain.DeleteScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenarioCatalog
import dev.proflyder.home.feature.scenarios.domain.SaveScenario
import dev.proflyder.home.feature.scenarios.presentation.ScenarioEditorViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ScenarioEditorViewModelTest {
    @After
    fun resetMainDispatcher() = Dispatchers.resetMain()

    private val entry = ControlEntry(
        id = "light.kitchen",
        kind = ControlKind.LIGHT,
        title = "Kitchen light",
        room = "Kitchen",
        icon = "",
        actions = setOf(ControlAction.SET, ControlAction.TURN_OFF),
        power = null,
        available = true,
    )

    @Test
    fun catalogLoadedBeforeScenarioIsNotErasedByScenarioState() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val viewModel = viewModel(catalogDelayMillis = 0, scenarioDelayMillis = 100)
        viewModel.start("sc-1")
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(1, state.catalog.size)
        assertFalse(state.catalogLoading)
        assertNull(state.catalogError)
        assertEquals("sc-1", state.draft.existingId)
    }

    @Test
    fun catalogLoadedAfterScenarioStillUpdatesTheSameState() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val viewModel = viewModel(catalogDelayMillis = 100, scenarioDelayMillis = 0)
        viewModel.start("sc-1")
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(1, state.catalog.size)
        assertFalse(state.catalogLoading)
        assertEquals("sc-1", state.draft.existingId)
    }

    @Test
    fun catalogErrorSurvivesScenarioLoad() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val viewModel = viewModel(
            catalogDelayMillis = 0,
            scenarioDelayMillis = 100,
            catalogFailure = dev.proflyder.home.core.model.RemoteFailure.Offline("no catalog"),
        )
        viewModel.start("sc-1")
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(0, state.catalog.size)
        assertFalse(state.catalogLoading)
        assertEquals("sc-1", state.draft.existingId)
        assertEquals(
            dev.proflyder.home.core.model.RemoteFailure.Offline("no catalog"),
            state.catalogError,
        )
    }

    private fun viewModel(
        catalogDelayMillis: Long,
        scenarioDelayMillis: Long,
        catalogFailure: dev.proflyder.home.core.model.RemoteFailure? = null,
    ): ScenarioEditorViewModel {
        val repository = FakeScenarioRepository(scenarioDelayMillis)
        return ScenarioEditorViewModel(
            loadScenario = LoadScenario(repository),
            loadCatalog = LoadScenarioCatalog(FakeCatalogClient(catalogDelayMillis, catalogFailure, entry)),
            saveScenario = SaveScenario(repository),
            deleteScenario = DeleteScenario(repository),
            settings = FakeSettings(),
            savedState = SavedStateHandle(),
        )
    }

    private class FakeScenarioRepository(private val delayMillis: Long) : ScenarioRepository {
        override suspend fun list(): Outcome<List<Scenario>> = Outcome.Success(listOf(scenario()))
        override suspend fun get(id: String): Outcome<Scenario> {
            if (delayMillis > 0) delay(delayMillis)
            return Outcome.Success(scenario())
        }
        override suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario> =
            Outcome.Success(scenario())
        override suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario> =
            Outcome.Success(scenario())
        override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario> =
            Outcome.Success(scenario())
        override suspend fun delete(id: String): Outcome<Boolean> = Outcome.Success(true)
        override suspend fun run(id: String, idempotencyKey: String): Outcome<String> =
            Outcome.Success("evt-1")
    }

    private class FakeCatalogClient(
        private val delayMillis: Long,
        private val failure: dev.proflyder.home.core.model.RemoteFailure?,
        private val entry: ControlEntry,
    ) : ControlCatalogClient {
        override suspend fun fetch(): Outcome<ControlCatalog> {
            if (delayMillis > 0) delay(delayMillis)
            failure?.let { return Outcome.Failure(it) }
            return Outcome.Success(ControlCatalog(listOf(entry), statesOk = true))
        }
    }

    private class FakeSettings : ConnectionSettingsStore {
        override val config: StateFlow<ConnectionConfig> = MutableStateFlow(ConnectionConfig())
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.ABSENT)
        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }

    private companion object {
        fun scenario() = Scenario(
            id = "sc-1",
            label = "Evening",
            description = "",
            enabled = true,
            revision = 3,
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

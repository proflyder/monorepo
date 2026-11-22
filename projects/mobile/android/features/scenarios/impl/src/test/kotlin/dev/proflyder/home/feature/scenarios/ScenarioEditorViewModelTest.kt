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
import dev.proflyder.home.core.designsystem.LightParamsEdit
import dev.proflyder.home.core.designsystem.apply
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.LightParams
import dev.proflyder.home.core.model.LightRanges
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.presentation.SharedFlowAppEventBus
import dev.proflyder.home.feature.scenarios.data.CatalogStepParams
import dev.proflyder.home.feature.scenarios.domain.ScenarioSaveSession
import dev.proflyder.home.feature.scenarios.presentation.editor.DelayForm
import dev.proflyder.home.feature.scenarios.presentation.editor.LightForm
import dev.proflyder.home.feature.scenarios.presentation.editor.SavedEditorState
import dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioEditorIntent
import dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioEditorViewModel
import dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioPrompt
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
import org.junit.Assert.assertTrue
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
        light = LightControls(brightness = true, colorTemp = false, color = false),
    )

    @Test
    fun catalogLoadedBeforeScenarioIsNotErasedByScenarioState() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val viewModel = viewModel(catalogDelayMillis = 0, scenarioDelayMillis = 100)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(1, state.catalog.entries.size)
        assertFalse(state.catalog.loading)
        assertNull(state.catalog.error)
        assertEquals("sc-1", state.draft.existingId)
    }

    @Test
    fun catalogLoadedAfterScenarioStillUpdatesTheSameState() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val viewModel = viewModel(catalogDelayMillis = 100, scenarioDelayMillis = 0)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(1, state.catalog.entries.size)
        assertFalse(state.catalog.loading)
        assertEquals("sc-1", state.draft.existingId)
    }

    @Test
    fun catalogErrorSurvivesScenarioLoad() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val viewModel = viewModel(
            catalogDelayMillis = 0,
            scenarioDelayMillis = 100,
            catalogFailure = RemoteFailure.Offline("no catalog"),
        )
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(0, state.catalog.entries.size)
        assertFalse(state.catalog.loading)
        assertEquals("sc-1", state.draft.existingId)
        assertEquals(RemoteFailure.Offline("no catalog"), state.catalog.error)
    }

    @Test
    fun openFormLivesInStateAndCommitsIntoTheDraft() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val viewModel = viewModel(catalogDelayMillis = 0, scenarioDelayMillis = 0)
        advanceUntilIdle()

        viewModel.dispatch(ScenarioEditorIntent.AddDelayStep)
        val opened = viewModel.state.value.editor as DelayForm
        viewModel.dispatch(ScenarioEditorIntent.UpdateForm(opened.copy(amount = "5")))
        viewModel.dispatch(ScenarioEditorIntent.CommitForm)

        val state = viewModel.state.value
        assertNull(state.editor)
        assertEquals(listOf(ScenarioStep.Delay(300)), state.draft.steps)
    }

    @Test
    fun lightStepIsEditedAsTypedParams() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val viewModel = viewModel(catalogDelayMillis = 0, scenarioDelayMillis = 0)
        advanceUntilIdle()

        viewModel.dispatch(ScenarioEditorIntent.AddCatalogStep)
        viewModel.dispatch(ScenarioEditorIntent.EntryPicked(entry.id))
        assertTrue(viewModel.state.value.prompt is ScenarioPrompt.ChooseAction)
        viewModel.dispatch(ScenarioEditorIntent.ActionChosen("set"))
        val form = viewModel.state.value.editor as LightForm
        assertEquals(LightRanges.MIN_BRIGHTNESS, form.editor.params.brightnessPct)
        val edited = form.copy(editor = form.editor.apply(LightParamsEdit.Brightness(40)).apply(LightParamsEdit.Transition("2")))
        viewModel.dispatch(ScenarioEditorIntent.UpdateForm(edited))
        viewModel.dispatch(ScenarioEditorIntent.CommitForm)

        val step = viewModel.state.value.draft.steps.single() as ScenarioStep.Catalog
        assertEquals("set", step.action)
        assertEquals(LightParams(brightnessPct = 40, transitionSeconds = 2.0), CatalogStepParams.lightParams(step.paramsJson))
    }

    @Test
    fun draftSurvivesRecreationFromSavedState() = runTest {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        val handle = SavedStateHandle()
        val first = viewModel(catalogDelayMillis = 0, scenarioDelayMillis = 0, handle = handle, scenarioId = null)
        advanceUntilIdle()
        first.dispatch(ScenarioEditorIntent.SetLabel("Night"))

        val second = viewModel(catalogDelayMillis = 0, scenarioDelayMillis = 0, handle = handle, scenarioId = null)
        advanceUntilIdle()
        assertEquals("Night", second.state.value.draft.label)
    }

    private fun viewModel(
        catalogDelayMillis: Long,
        scenarioDelayMillis: Long,
        catalogFailure: RemoteFailure? = null,
        handle: SavedStateHandle = SavedStateHandle(),
        scenarioId: String? = "sc-1",
    ): ScenarioEditorViewModel {
        scenarioId?.let { handle[ScenarioEditorDestination.ARG_SCENARIO_ID] = it }
        val repository = FakeScenarioRepository(scenarioDelayMillis)
        val saved = SavedEditorState(handle)
        return ScenarioEditorViewModel(
            loadScenario = LoadScenario(repository),
            loadCatalog = LoadScenarioCatalog(FakeCatalogClient(catalogDelayMillis, catalogFailure, entry)),
            deleteScenario = DeleteScenario(repository),
            appEvents = SharedFlowAppEventBus(),
            saved = saved,
            session = ScenarioSaveSession(SaveScenario(repository), FakeSettings(), saved),
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
        private val failure: RemoteFailure?,
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

package dev.proflyder.home.feature.scenarios.presentation

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.isAmbiguousRetryable
import dev.proflyder.home.core.operation.EditorCompletion
import dev.proflyder.home.core.operation.IdempotentRequestKey
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.core.model.ScenarioLimits
import dev.proflyder.home.feature.scenarios.ScenarioEditorCompletion
import dev.proflyder.home.feature.scenarios.ScenarioEditorDraft
import dev.proflyder.home.feature.scenarios.ScenarioEditorDraftCodec
import dev.proflyder.home.feature.scenarios.ScenarioValidation
import dev.proflyder.home.feature.scenarios.domain.DeleteScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenarioCatalog
import dev.proflyder.home.feature.scenarios.domain.SaveScenario
import dev.proflyder.home.feature.scenarios.isScenarioConflict
import dev.proflyder.home.feature.scenarios.toScenarioDraft
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import java.time.LocalDateTime
import java.time.ZoneId
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

data class ScenarioEditorUiState(
    val loading: Boolean = false,
    val saving: Boolean = false,
    val existing: Scenario? = null,
    val draft: ScenarioEditorDraft = ScenarioEditorDraft(),
    val catalog: List<ControlEntry> = emptyList(),
    val catalogLoading: Boolean = false,
    val catalogError: RemoteFailure? = null,
    val error: RemoteFailure? = null,
    val loadFailed: Boolean = false,
    /** Set when UpdateScenario was rejected because the server has a newer revision. */
    val conflict: Boolean = false,
) {
    val hasUnsupportedParts: Boolean get() = draft.hasUnsupportedParts
}

class ScenarioEditorViewModel(
    private val loadScenario: LoadScenario,
    private val loadCatalog: LoadScenarioCatalog,
    private val saveScenario: SaveScenario,
    private val deleteScenario: DeleteScenario,
    private val settings: ConnectionSettingsStore,
    private val savedState: SavedStateHandle,
) : BaseViewModel<ScenarioEditorUiState>(ScenarioEditorUiState()) {
    private val restoredDraft: ScenarioEditorDraft? = ScenarioEditorDraftCodec.fromMap(
        savedState.keys().associateWith { key -> savedState.get<Any?>(key) },
    )

    private val editorCompletion = EditorCompletion(
        savedState.get<String>(KEY_COMPLETION)?.let { raw ->
            ScenarioEditorCompletion.entries.firstOrNull { it.name == raw }
        },
    )
    val completion: StateFlow<ScenarioEditorCompletion?> = editorCompletion.state

    private val createIdentity = IdempotentRequestKey<ScenarioDraft>()
    private var started = false

    init {
        val savedKey = savedState.get<String>(KEY_CREATE_KEY)
        val savedEndpoint = savedState.get<String>(KEY_CREATE_ENDPOINT)
        val currentEndpoint = settings.config.value.endpoint?.baseUrl().orEmpty()
        adoptRestoredCreateIdentity(savedKey, savedEndpoint, currentEndpoint)
        if (restoredDraft != null) {
            mutableState.value = mutableState.value.copy(
                existing = restoredDraft.restoredExisting(),
                draft = restoredDraft,
            )
        }
    }

    private fun adoptRestoredCreateIdentity(
        savedKey: String?,
        savedEndpoint: String?,
        currentEndpoint: String,
    ) {
        val draft = restoredDraft ?: return
        if (draft.existingId != null) return
        if (savedKey == null || savedEndpoint != currentEndpoint) return
        createIdentity.adopt(draft.toScenarioDraft(), savedKey)
    }

    fun start(scenarioId: String?) {
        if (started) return
        started = true
        refreshCatalog()
        if (restoredDraft != null) return
        if (scenarioId == null) {
            persistDraft()
            return
        }
        mutableState.value = mutableState.value.copy(loading = true, error = null)
        viewModelScope.launch {
            when (val result = loadScenario(scenarioId)) {
                is Outcome.Success -> {
                    val draft = ScenarioEditorDraftCodec.fromScenario(result.value)
                    // Keep the independently loaded catalog: replacing the
                    // whole state here used to erase catalog/catalogLoading/
                    // catalogError and made step picking say "catalog empty".
                    mutableState.value = mutableState.value.copy(
                        loading = false,
                        existing = result.value,
                        draft = draft,
                        error = null,
                        loadFailed = false,
                    )
                    persistDraft()
                }
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    loading = false,
                    error = result.failure,
                    loadFailed = true,
                )
            }
        }
    }

    fun reloadCatalog() = refreshCatalog()

    private fun refreshCatalog() {
        if (mutableState.value.catalogLoading) return
        mutableState.value = mutableState.value.copy(catalogLoading = true, catalogError = null)
        viewModelScope.launch {
            when (val result = loadCatalog()) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    catalogLoading = false,
                    catalog = result.value.entries,
                    catalogError = null,
                )
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    catalogLoading = false,
                    catalogError = result.failure,
                )
            }
        }
    }

    fun setLabel(value: String) = editDraft { it.copy(label = value) }

    fun setDescription(value: String) = editDraft { it.copy(description = value) }

    fun setEnabled(value: Boolean) = editDraft { it.copy(enabled = value) }

    fun setContinueOnError(value: Boolean) = editDraft { it.copy(continueOnError = value) }

    fun addTimeTrigger(trigger: TimeTrigger = defaultTimeTrigger()) = editDraft {
        if (it.triggers.size >= ScenarioLimits.MAX_TRIGGERS) it
        else it.copy(triggers = it.triggers + ScenarioTrigger.Time(trigger))
    }

    fun updateTimeTrigger(index: Int, trigger: TimeTrigger) = editDraft {
        if (index !in it.triggers.indices) return@editDraft it
        it.copy(triggers = it.triggers.toMutableList().also { list -> list[index] = ScenarioTrigger.Time(trigger) })
    }

    fun removeTrigger(index: Int) = editDraft {
        if (index !in it.triggers.indices) it
        else it.copy(triggers = it.triggers.filterIndexed { i, _ -> i != index })
    }

    fun addCondition(condition: ScenarioCondition = ScenarioCondition.EntityState("", "on")) = editDraft {
        if (it.conditions.size >= ScenarioLimits.MAX_CONDITIONS) it
        else it.copy(conditions = it.conditions + condition)
    }

    fun updateCondition(index: Int, condition: ScenarioCondition) = editDraft {
        if (index !in it.conditions.indices) return@editDraft it
        it.copy(conditions = it.conditions.toMutableList().also { list -> list[index] = condition })
    }

    fun removeCondition(index: Int) = editDraft {
        if (index !in it.conditions.indices) it
        else it.copy(conditions = it.conditions.filterIndexed { i, _ -> i != index })
    }

    fun addStep(step: ScenarioStep) = editDraft {
        if (it.steps.size >= ScenarioLimits.MAX_STEPS) it else it.copy(steps = it.steps + step)
    }

    fun updateStep(index: Int, step: ScenarioStep) = editDraft {
        if (index !in it.steps.indices) return@editDraft it
        it.copy(steps = it.steps.toMutableList().also { list -> list[index] = step })
    }

    fun removeStep(index: Int) = editDraft {
        if (index !in it.steps.indices) it
        else it.copy(steps = it.steps.filterIndexed { i, _ -> i != index })
    }

    /** Moves a step one position up ([delta] = -1) or down ([delta] = 1). */
    fun moveStep(index: Int, delta: Int) = editDraft { moveStepIn(it, index, delta) }

    private fun moveStepIn(draft: ScenarioEditorDraft, index: Int, delta: Int): ScenarioEditorDraft {
        val target = index + delta
        if (index !in draft.steps.indices || target !in draft.steps.indices) return draft
        val list = draft.steps.toMutableList()
        val moved = list.removeAt(index)
        list.add(target, moved)
        return draft.copy(steps = list)
    }

    fun save() {
        val current = mutableState.value
        if (current.saving) return
        val draft = current.draft
        if (draft.hasUnsupportedParts) {
            mutableState.value = current.copy(error = RemoteFailure.Validation(Field.UNSUPPORTED))
            return
        }
        val payload = draft.toScenarioDraft()
        ScenarioValidation.validate(payload)?.let { failure ->
            mutableState.value = current.copy(error = failure)
            return
        }
        val existing = current.existing ?: draft.restoredExisting()
        val endpoint = settings.config.value.endpoint?.baseUrl().orEmpty()
        val key = if (existing == null) {
            createIdentity.keyFor(payload).also {
                savedState[KEY_CREATE_KEY] = it
                savedState[KEY_CREATE_ENDPOINT] = endpoint
            }
        } else {
            clearPendingKey()
            ""
        }
        mutableState.value = current.copy(saving = true, error = null, conflict = false)
        viewModelScope.launch {
            when (val result = saveScenario(existing, payload, key)) {
                is Outcome.Success -> {
                    createIdentity.markSucceeded()
                    clearPendingKey()
                    val draft = ScenarioEditorDraftCodec.fromScenario(result.value)
                    mutableState.value = mutableState.value.copy(
                        saving = false,
                        existing = result.value,
                        draft = draft,
                        error = null,
                        conflict = false,
                    )
                    persistDraft()
                    setCompletion(ScenarioEditorCompletion.SAVED)
                }
                is Outcome.Failure -> {
                    if (result.failure.isScenarioConflict()) {
                        mutableState.value = mutableState.value.copy(
                            saving = false,
                            error = result.failure,
                            conflict = true,
                        )
                        return@launch
                    }
                    if (!result.failure.isAmbiguousRetryable()) {
                        createIdentity.reset()
                        clearPendingKey()
                    }
                    mutableState.value = mutableState.value.copy(
                        saving = false,
                        error = result.failure,
                    )
                }
            }
        }
    }

    /** Re-reads the server definition after a revision conflict, discarding local edits. */
    fun reload() {
        val id = mutableState.value.existing?.id ?: mutableState.value.draft.existingId ?: return
        mutableState.value = mutableState.value.copy(loading = true, error = null, conflict = false)
        viewModelScope.launch {
            when (val result = loadScenario(id)) {
                is Outcome.Success -> {
                    val draft = ScenarioEditorDraftCodec.fromScenario(result.value)
                    mutableState.value = mutableState.value.copy(
                        loading = false,
                        existing = result.value,
                        draft = draft,
                        error = null,
                        conflict = false,
                    )
                    persistDraft()
                }
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    loading = false,
                    error = result.failure,
                )
            }
        }
    }

    fun delete() {
        val existing = mutableState.value.existing ?: return
        if (mutableState.value.saving) return
        mutableState.value = mutableState.value.copy(saving = true, error = null)
        viewModelScope.launch {
            when (val result = deleteScenario(existing.id)) {
                is Outcome.Success -> {
                    mutableState.value = mutableState.value.copy(saving = false, error = null)
                    setCompletion(ScenarioEditorCompletion.DELETED)
                }
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    saving = false,
                    error = result.failure,
                )
            }
        }
    }

    fun consumeCompletion() {
        editorCompletion.consume()
        savedState.remove<String>(KEY_COMPLETION)
    }

    private fun editDraft(transform: (ScenarioEditorDraft) -> ScenarioEditorDraft) {
        createIdentity.reset()
        clearPendingKey()
        mutableState.value = mutableState.value.copy(
            draft = transform(mutableState.value.draft),
            error = null,
            conflict = false,
        )
        persistDraft()
    }

    private fun clearPendingKey() {
        savedState.remove<String>(KEY_CREATE_KEY)
        savedState.remove<String>(KEY_CREATE_ENDPOINT)
    }

    private fun setCompletion(completion: ScenarioEditorCompletion) {
        editorCompletion.publish(completion)
        savedState[KEY_COMPLETION] = completion.name
    }

    private fun persistDraft() {
        ScenarioEditorDraftCodec.toMap(mutableState.value.draft).forEach { (key, value) ->
            savedState[key] = value
        }
    }

    companion object {
        private const val KEY_COMPLETION = "scenario_completion"
        private const val KEY_CREATE_KEY = "scenario_create_key"
        private const val KEY_CREATE_ENDPOINT = "scenario_create_endpoint"

        /** Next full hour in the phone's IANA zone, a time that is not already past. */
        internal fun defaultTimeTrigger(now: LocalDateTime = LocalDateTime.now()): TimeTrigger {
            val next = now.plusHours(1)
            return TimeTrigger(
                timezone = ZoneId.systemDefault().id,
                at = WallTime(next.hour, next.minute),
                repeat = RepeatKind.DAILY,
            )
        }

        internal fun defaultTimeWindow(): ScenarioCondition.TimeWindow =
            ScenarioCondition.TimeWindow(
                start = WallTime(8, 0),
                end = WallTime(22, 0),
                weekdays = emptyList(),
            )
    }
}

/** Reconstructs enough of the server object for an Update after process recreation. */
internal fun ScenarioEditorDraft.restoredExisting(): Scenario? {
    val id = existingId ?: return null
    return Scenario(
        id = id,
        label = label,
        description = description,
        enabled = enabled,
        revision = existingRevision ?: 0,
        createdUnix = 0,
        updatedUnix = 0,
        triggers = triggers,
        conditions = conditions,
        steps = steps,
        continueOnError = continueOnError,
        nextDueUnix = 0,
    )
}

/** Best-effort entry lookup for editor card titles. */
internal fun List<ControlEntry>.titleOf(entryId: String): String? =
    firstOrNull { it.id == entryId }?.let { it.title.ifBlank { it.id } }

@ContributesIntoSet(AppScope::class)
@Inject
class ScenarioEditorViewModelFactoryContributor(
    private val loadScenario: LoadScenario,
    private val loadCatalog: LoadScenarioCatalog,
    private val saveScenario: SaveScenario,
    private val deleteScenario: DeleteScenario,
    private val settings: ConnectionSettingsStore,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ScenarioEditorViewModel::class.java

    override fun createFactory() = SavedStateViewModelFactory { savedState ->
        ScenarioEditorViewModel(
            loadScenario = loadScenario,
            loadCatalog = loadCatalog,
            saveScenario = saveScenario,
            deleteScenario = deleteScenario,
            settings = settings,
            savedState = savedState,
        )
    }
}

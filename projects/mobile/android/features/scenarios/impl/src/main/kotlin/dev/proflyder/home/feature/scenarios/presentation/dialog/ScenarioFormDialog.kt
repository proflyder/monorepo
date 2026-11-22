package dev.proflyder.home.feature.scenarios.presentation.dialog

import androidx.fragment.app.viewModels
import androidx.lifecycle.lifecycleScope
import dev.proflyder.home.core.designsystem.EditorDialogFragment
import dev.proflyder.home.core.designsystem.EditorSession
import dev.proflyder.home.core.ui.injectedViewModelFactory
import dev.proflyder.home.feature.scenarios.presentation.editor.EditorForm
import dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioEditorIntent
import dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioEditorUiState
import dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioEditorViewModel
import kotlin.reflect.KClass
import kotlin.reflect.safeCast
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

internal abstract class ScenarioFormDialog<F : EditorForm>(private val type: KClass<F>) : EditorDialogFragment<F>() {
    private val editor: ScenarioEditorViewModel by viewModels(
        ownerProducer = { requireParentFragment() },
        factoryProducer = { requireContext().injectedViewModelFactory() },
    )

    override val session: EditorSession<F> by lazy { FormSession(editor, type, lifecycleScope) }

    protected val editorState: ScenarioEditorUiState get() = editor.state.value

    protected fun edit(transform: (F) -> F) {
        session.value.value?.let { editor.dispatch(ScenarioEditorIntent.UpdateForm(transform(it))) }
    }

    protected fun dispatch(intent: ScenarioEditorIntent) = editor.dispatch(intent)

    companion object {
        fun tagOf(form: EditorForm): String = "scenario_form_${form::class.simpleName}"
    }
}

private class FormSession<F : EditorForm>(
    private val editor: ScenarioEditorViewModel,
    type: KClass<F>,
    scope: CoroutineScope,
) : EditorSession<F> {
    override val value: StateFlow<F?> = editor.state
        .map { type.safeCast(it.editor) }
        .stateIn(scope, SharingStarted.Eagerly, type.safeCast(editor.state.value.editor))

    override fun canCommit(value: F): Boolean = value.isValid

    override fun commit() = editor.dispatch(ScenarioEditorIntent.CommitForm)

    override fun cancel() = editor.dispatch(ScenarioEditorIntent.CancelForm)
}

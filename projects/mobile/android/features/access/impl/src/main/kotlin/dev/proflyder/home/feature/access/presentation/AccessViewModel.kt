package dev.proflyder.home.feature.access.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.access.AccessRepository
import dev.proflyder.home.feature.access.AccessTokensState
import dev.proflyder.home.feature.access.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class CreatedAccessState(val name: String, val token: String)

private const val MAX_NAME = 64

internal data class AccessEditorState(
    val id: String?,
    val owner: Boolean,
    val name: String = "",
    val modules: Set<AppModule> = emptySet(),
    val saving: Boolean = false,
    val nameError: Boolean = false,
    val errorRes: Int? = null,
) {
    val editable: Boolean get() = !owner

    val canSave: Boolean get() = editable && name.trim().isNotEmpty() && name.trim().length <= MAX_NAME && !saving
}

internal data class AccessUiState(
    val tokens: AccessTokensState = AccessTokensState(),
    val editor: AccessEditorState? = null,
    val created: CreatedAccessState? = null,
)

internal sealed interface AccessIntent {
    data object Refresh : AccessIntent
    data object New : AccessIntent
    data class Edit(val token: AccessToken) : AccessIntent
    data class RevokeRequest(val token: AccessToken) : AccessIntent
    data class Revoke(val id: String) : AccessIntent
    data class EditorName(val value: String) : AccessIntent
    data class EditorModule(val module: AppModule, val enabled: Boolean) : AccessIntent
    data object EditorSave : AccessIntent
    data object EditorDismiss : AccessIntent
    data object CreatedDismiss : AccessIntent
}

internal sealed interface AccessEvent {
    data class ConfirmRevoke(val token: AccessToken) : AccessEvent
    data class Failure(val failure: RemoteFailure) : AccessEvent
}

@Inject
internal class AccessViewModel(
    private val repository: AccessRepository,
) : StateViewModel<AccessUiState, AccessIntent, AccessEvent>(AccessUiState()) {
    init {
        repository.state.onEach { tokens -> updateState { copy(tokens = tokens) } }.launchIn(viewModelScope)
        repository.refresh()
    }

    override fun handle(intent: AccessIntent) {
        when (intent) {
            AccessIntent.Refresh -> repository.refresh()
            AccessIntent.New -> updateState { copy(editor = newEditor()) }
            is AccessIntent.Edit -> updateState { copy(editor = editorOf(intent.token)) }
            is AccessIntent.RevokeRequest -> emit(AccessEvent.ConfirmRevoke(intent.token))
            is AccessIntent.Revoke -> revoke(intent.id)
            is AccessIntent.EditorName -> rename(intent.value)
            is AccessIntent.EditorModule -> toggle(intent.module, intent.enabled)
            AccessIntent.EditorSave -> save()
            AccessIntent.EditorDismiss -> updateState { copy(editor = null) }
            AccessIntent.CreatedDismiss -> updateState { copy(created = null) }
        }
    }

    private fun newEditor(): AccessEditorState = AccessEditorState(id = null, owner = false)

    private fun editorOf(token: AccessToken): AccessEditorState = AccessEditorState(
        id = token.id,
        owner = token.owner,
        name = token.name,
        modules = token.modules,
    )

    private fun rename(value: String) {
        val editor = currentState.editor ?: return
        if (!editor.editable || editor.saving) return
        updateState { copy(editor = editor.copy(name = value, nameError = false, errorRes = null)) }
    }

    private fun toggle(module: AppModule, enabled: Boolean) {
        val editor = currentState.editor ?: return
        if (!editor.editable || editor.saving) return
        val modules = if (enabled) editor.modules + module else editor.modules - module
        updateState { copy(editor = editor.copy(modules = modules, errorRes = null)) }
    }

    private fun save() {
        val editor = currentState.editor ?: return
        if (!editor.canSave) {
            updateState { copy(editor = editor.copy(nameError = editor.name.trim().isEmpty())) }
            return
        }
        updateState { copy(editor = editor.copy(saving = true, nameError = false, errorRes = null)) }
        if (editor.id == null) create(editor) else update(editor)
    }

    private fun create(editor: AccessEditorState) {
        launch {
            val result = repository.create(editor.name.trim(), editor.modules)
            when (result) {
                is Outcome.Success -> updateState {
                    copy(editor = null, created = CreatedAccessState(result.value.access.name, result.value.token))
                }
                is Outcome.Failure -> fail(editor, result.failure)
            }
        }
    }

    private fun update(editor: AccessEditorState) {
        val id = editor.id ?: return
        launch {
            val result = repository.update(id, editor.name.trim(), editor.modules)
            when (result) {
                is Outcome.Success -> updateState { copy(editor = null) }
                is Outcome.Failure -> fail(editor, result.failure)
            }
        }
    }

    private fun revoke(id: String) {
        launch { repository.revoke(id).failureOrNull()?.let { emit(AccessEvent.Failure(it)) } }
    }

    private fun fail(editor: AccessEditorState, failure: RemoteFailure) {
        val errorRes = editorErrorRes(failure)
        updateState { copy(editor = editor.copy(saving = false, errorRes = errorRes)) }
        if (errorRes == null) emit(AccessEvent.Failure(failure))
    }
}

internal fun editorErrorRes(failure: RemoteFailure): Int? = when {
    failure !is RemoteFailure.Server -> null
    failure.httpStatus == 400 -> R.string.access_error_invalid
    failure.httpStatus == 409 -> R.string.access_error_name_taken
    failure.httpStatus == 429 -> R.string.access_error_limit
    else -> null
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(AccessViewModel::class)
@Inject
class AccessViewModelSource internal constructor(
    private val viewModel: () -> AccessViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

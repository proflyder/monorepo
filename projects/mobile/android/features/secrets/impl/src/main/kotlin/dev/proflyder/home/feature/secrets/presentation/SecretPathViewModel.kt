package dev.proflyder.home.feature.secrets.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.secrets.SecretsRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

internal enum class SecretActionKind { REVEAL, UPDATE, ROLLBACK, DELETE }

internal data class SecretAction(
    val kind: SecretActionKind,
    val key: String? = null,
    val value: String = "",
    val version: Long? = null,
    val copy: Boolean = false,
)

internal sealed interface SecretDialog {
    data class StepUp(
        val action: SecretAction,
        val digits: String = "",
        val busy: Boolean = false,
        val problem: SecretProblem? = null,
    ) : SecretDialog {
        val complete: Boolean get() = digits.length == TOTP_LENGTH && digits.all(Char::isDigit)
    }

    data class Edit(
        val key: String,
        val value: String = "",
        val busy: Boolean = false,
        val problem: SecretProblem? = null,
    ) : SecretDialog

    data class Confirm(val action: SecretAction) : SecretDialog
}

internal data class SecretPathUiState(
    val path: String = "",
    val description: String = "",
    val metadata: SecretMetadata? = null,
    val loading: Boolean = false,
    val failure: RemoteFailure? = null,
    val revealedKey: String? = null,
    val revealedVersion: Long? = null,
    val revealedValue: String? = null,
    val revealingKey: String? = null,
    val sessionActive: Boolean = false,
    val dialog: SecretDialog? = null,
)

internal sealed interface SecretPathIntent {
    data class SetPath(val path: String) : SecretPathIntent
    data object Refresh : SecretPathIntent
    data class Reveal(val key: String) : SecretPathIntent
    data class Copy(val key: String) : SecretPathIntent
    data object ReMask : SecretPathIntent
    data class EditRequest(val key: String) : SecretPathIntent
    data class EditValue(val value: String) : SecretPathIntent
    data object EditSubmit : SecretPathIntent
    data class RollbackRequest(val version: Long) : SecretPathIntent
    data object DeleteRequest : SecretPathIntent
    data object ConfirmAccept : SecretPathIntent
    data object DialogDismiss : SecretPathIntent
    data class StepUpDigits(val value: String) : SecretPathIntent
    data object StepUpSubmit : SecretPathIntent
}

internal sealed interface SecretPathEvent {
    data class CopyValue(val value: String) : SecretPathEvent
    data class Message(val problem: SecretProblem) : SecretPathEvent
    data object Deleted : SecretPathEvent
}

@Inject
internal class SecretPathViewModel(
    private val repository: SecretsRepository,
) : StateViewModel<SecretPathUiState, SecretPathIntent, SecretPathEvent>(SecretPathUiState()) {
    private var mask: Job? = null

    init {
        repository.stepUpActive
            .onEach { active -> updateState { copy(sessionActive = active) } }
            .launchIn(viewModelScope)
    }

    override fun handle(intent: SecretPathIntent) {
        when (intent) {
            is SecretPathIntent.SetPath -> setPath(intent.path)
            SecretPathIntent.Refresh -> load()
            is SecretPathIntent.Reveal -> reveal(intent.key, copy = false)
            is SecretPathIntent.Copy -> reveal(intent.key, copy = true)
            SecretPathIntent.ReMask -> reMask()
            is SecretPathIntent.EditRequest -> editRequest(intent.key)
            is SecretPathIntent.EditValue -> editValue(intent.value)
            SecretPathIntent.EditSubmit -> editSubmit()
            is SecretPathIntent.RollbackRequest -> confirm(
                SecretAction(SecretActionKind.ROLLBACK, version = intent.version),
            )
            SecretPathIntent.DeleteRequest -> currentVersion()?.let { version ->
                confirm(SecretAction(SecretActionKind.DELETE, version = version))
            }
            SecretPathIntent.ConfirmAccept -> confirmAccept()
            SecretPathIntent.DialogDismiss -> updateState { copy(dialog = null) }
            is SecretPathIntent.StepUpDigits -> stepUpDigits(intent.value)
            SecretPathIntent.StepUpSubmit -> stepUpSubmit()
        }
    }

    private fun setPath(path: String) {
        if (path.isBlank() || path == currentState.path) return
        mask?.cancel()
        updateState {
            SecretPathUiState(
                path = path,
                description = repository.catalog.value.entries.firstOrNull { it.path == path }?.description.orEmpty(),
                sessionActive = repository.stepUpActive.value,
            )
        }
        load()
    }

    private fun load() {
        val path = currentState.path
        if (path.isBlank()) return
        updateState { copy(loading = true, failure = null) }
        launch {
            when (val result = repository.metadata(path)) {
                is Outcome.Success -> updateState {
                    copy(
                        loading = false,
                        metadata = result.value,
                        failure = null,
                        revealedKey = revealedKey?.takeIf { key -> key in result.value.keys },
                        description = description.ifEmpty { repositoryCatalogDescription(path) },
                    )
                }
                is Outcome.Failure -> updateState { copy(loading = false, failure = result.failure) }
            }
        }
    }

    private fun repositoryCatalogDescription(path: String): String =
        repository.catalog.value.entries.firstOrNull { it.path == path }?.description.orEmpty()

    private fun reveal(key: String, copy: Boolean) {
        val version = currentVersion() ?: return
        if (key == currentState.revealedKey && currentState.revealedVersion == version && !copy) {
            reMask()
            return
        }
        val action = SecretAction(SecretActionKind.REVEAL, key = key, version = version, copy = copy)
        if (currentState.sessionActive) {
            performReveal(action)
            return
        }
        updateState { copy(dialog = SecretDialog.StepUp(action)) }
    }

    private fun performReveal(action: SecretAction) {
        val key = action.key ?: return
        val version = action.version ?: return
        updateState { copy(revealingKey = key) }
        launch {
            when (val result = repository.reveal(currentState.path, key, version)) {
                is Outcome.Success -> {
                    updateState {
                        copy(
                            revealingKey = null,
                            revealedKey = result.value.key,
                            revealedVersion = result.value.version,
                            revealedValue = result.value.value,
                            dialog = null,
                        )
                    }
                    scheduleMask()
                    if (action.copy) emit(SecretPathEvent.CopyValue(result.value.value))
                }
                is Outcome.Failure -> revealFailed(result.failure)
            }
        }
    }

    private fun revealFailed(failure: RemoteFailure) {
        val problem = secretProblem(failure)
        val key = currentState.revealingKey
        updateState { copy(revealingKey = null) }
        if (problem == SecretProblem.StepUpRequired && key != null) {
            val version = currentVersion() ?: return
            updateState {
                copy(
                    revealedKey = null,
                    revealedValue = null,
                    dialog = SecretDialog.StepUp(
                        SecretAction(SecretActionKind.REVEAL, key = key, version = version),
                    ),
                )
            }
            return
        }
        emit(SecretPathEvent.Message(problem))
    }

    private fun reMask() {
        mask?.cancel()
        updateState { copy(revealedKey = null, revealedVersion = null, revealedValue = null) }
    }

    private fun scheduleMask() {
        mask?.cancel()
        mask = viewModelScope.launch {
            delay(RE_MASK_MILLIS)
            updateState { copy(revealedKey = null, revealedVersion = null, revealedValue = null) }
        }
    }

    private fun editRequest(key: String) {
        updateState { copy(dialog = SecretDialog.Edit(key)) }
    }

    private fun editValue(value: String) {
        val edit = currentState.dialog as? SecretDialog.Edit ?: return
        updateState { copy(dialog = edit.copy(value = value, problem = null)) }
    }

    private fun editSubmit() {
        val edit = currentState.dialog as? SecretDialog.Edit ?: return
        if (edit.busy) return
        val action = SecretAction(SecretActionKind.UPDATE, key = edit.key, value = edit.value)
        updateState { copy(dialog = SecretDialog.StepUp(action)) }
    }

    private fun confirm(action: SecretAction) {
        updateState { copy(dialog = SecretDialog.Confirm(action)) }
    }

    private fun confirmAccept() {
        val confirm = currentState.dialog as? SecretDialog.Confirm ?: return
        updateState { copy(dialog = SecretDialog.StepUp(confirm.action)) }
    }

    private fun stepUpDigits(value: String) {
        val stepUp = currentState.dialog as? SecretDialog.StepUp ?: return
        val digits = value.filter(Char::isDigit).take(TOTP_LENGTH)
        updateState { copy(dialog = stepUp.copy(digits = digits, problem = null)) }
    }

    private fun stepUpSubmit() {
        val stepUp = currentState.dialog as? SecretDialog.StepUp ?: return
        if (!stepUp.complete || stepUp.busy) return
        if (stepUp.action.kind == SecretActionKind.REVEAL) {
            openSession(stepUp)
            return
        }
        mutate(stepUp)
    }

    private fun openSession(stepUp: SecretDialog.StepUp) {
        updateState { copy(dialog = stepUp.copy(busy = true, problem = null)) }
        launch {
            when (val result = repository.stepUp(stepUp.digits)) {
                is Outcome.Success -> {
                    updateState { copy(dialog = stepUp.copy(busy = false, digits = "")) }
                    performReveal(stepUp.action)
                }
                is Outcome.Failure -> stepUpFailed(stepUp, result.failure)
            }
        }
    }

    private fun mutate(stepUp: SecretDialog.StepUp) {
        val cas = currentVersion() ?: return
        updateState { copy(dialog = stepUp.copy(busy = true, problem = null)) }
        launch {
            val action = stepUp.action
            when (action.kind) {
                SecretActionKind.UPDATE -> mutationResult(
                    stepUp,
                    repository.update(
                        currentState.path,
                        action.key.orEmpty(),
                        action.value,
                        cas,
                        stepUp.digits,
                    ),
                )
                SecretActionKind.ROLLBACK -> mutationResult(
                    stepUp,
                    repository.rollback(
                        currentState.path,
                        action.version ?: return@launch,
                        cas,
                        stepUp.digits,
                    ),
                )
                SecretActionKind.DELETE -> mutationResult(
                    stepUp,
                    repository.delete(
                        currentState.path,
                        cas,
                        action.version ?: return@launch,
                        stepUp.digits,
                    ),
                )
                SecretActionKind.REVEAL -> Unit
            }
        }
    }

    private fun mutationResult(stepUp: SecretDialog.StepUp, result: Outcome<*>) {
        when (result) {
            is Outcome.Success -> mutationSucceeded(stepUp.action.kind)
            is Outcome.Failure -> stepUpFailed(stepUp, result.failure)
        }
    }

    private fun mutationSucceeded(kind: SecretActionKind) {
        updateState {
            copy(
                dialog = null,
                revealedKey = null,
                revealedVersion = null,
                revealedValue = null,
                failure = null,
            )
        }
        when (kind) {
            SecretActionKind.DELETE -> emit(SecretPathEvent.Deleted)
            else -> load()
        }
    }

    private fun stepUpFailed(stepUp: SecretDialog.StepUp, failure: RemoteFailure) {
        val problem = secretProblem(failure)
        when (problem) {
            SecretProblem.StepUpRequired -> updateState {
                copy(
                    dialog = stepUp.copy(busy = false, problem = SecretProblem.StepUpRequired),
                    sessionActive = false,
                )
            }
            SecretProblem.InvalidTotp -> updateState {
                copy(dialog = stepUp.copy(busy = false, digits = "", problem = problem))
            }
            else -> {
                updateState { copy(dialog = stepUp.copy(busy = false, problem = problem)) }
                emit(SecretPathEvent.Message(problem))
            }
        }
    }

    private fun currentVersion(): Long? = currentState.metadata?.currentVersion

    private companion object {
        const val RE_MASK_MILLIS = 30_000L
    }
}

private const val TOTP_LENGTH = 6

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(SecretPathViewModel::class)
@Inject
class SecretPathViewModelSource internal constructor(
    private val viewModel: () -> SecretPathViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

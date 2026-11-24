package dev.proflyder.home.feature.secrets.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretOperation
import dev.proflyder.home.core.model.StepUpChallenge
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
        val busy: Boolean = false,
        val problem: SecretProblem? = null,
    ) : SecretDialog

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
    data object StepUpSubmit : SecretPathIntent
    data class StepUpSigned(val signature: String) : SecretPathIntent
    data object StepUpCancelled : SecretPathIntent
    data class StepUpFailed(val reason: DeviceSignatureFailure) : SecretPathIntent
}

internal sealed interface SecretPathEvent {
    data class CopyValue(val value: String) : SecretPathEvent
    data class Message(val problem: SecretProblem) : SecretPathEvent
    data class Sign(
        val challenge: StepUpChallenge,
        val operation: SecretOperation,
        val action: SecretAction,
    ) : SecretPathEvent
    data object Deleted : SecretPathEvent
}

@Inject
internal class SecretPathViewModel(
    private val repository: SecretsRepository,
) : StateViewModel<SecretPathUiState, SecretPathIntent, SecretPathEvent>(SecretPathUiState()) {
    private var mask: Job? = null
    private var signing: PendingSignature? = null

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
            SecretPathIntent.DialogDismiss -> dismiss()
            SecretPathIntent.StepUpSubmit -> stepUpSubmit()
            is SecretPathIntent.StepUpSigned -> signed(intent.signature)
            SecretPathIntent.StepUpCancelled -> cancelled()
            is SecretPathIntent.StepUpFailed -> signingFailed(intent.reason)
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

    private fun dismiss() {
        signing = null
        updateState { copy(dialog = null) }
    }

    private fun stepUpSubmit() {
        val stepUp = currentState.dialog as? SecretDialog.StepUp ?: return
        if (stepUp.busy) return
        val version = currentVersion() ?: return
        val operation = operationFor(stepUp.action)
        updateState { copy(dialog = stepUp.copy(busy = true, problem = null)) }
        launch {
            val cas = if (operation == SecretOperation.SESSION) null else version
            when (val result = repository.challenge(operation, pathFor(operation), keyFor(stepUp.action), versionFor(stepUp.action), cas)) {
                is Outcome.Success -> {
                    signing = PendingSignature(stepUp.action, operation, result.value)
                    updateState { copy(dialog = stepUp.copy(busy = false)) }
                    emit(SecretPathEvent.Sign(result.value, operation, stepUp.action))
                }
                is Outcome.Failure -> stepUpFailed(stepUp, result.failure)
            }
        }
    }

    private fun signed(signature: String) {
        val pending = signing ?: return
        signing = null
        val stepUp = currentState.dialog as? SecretDialog.StepUp ?: return
        updateState { copy(dialog = stepUp.copy(busy = true, problem = null)) }
        launch {
            if (pending.operation == SecretOperation.SESSION) {
                openSession(stepUp, pending, signature)
                return@launch
            }
            mutate(stepUp, pending, signature)
        }
    }

    private suspend fun openSession(
        stepUp: SecretDialog.StepUp,
        pending: PendingSignature,
        signature: String,
    ) {
        when (val result = repository.stepUp(pending.challenge.challenge, signature)) {
            is Outcome.Success -> {
                updateState { copy(dialog = null, sessionActive = true) }
                performReveal(pending.action)
            }
            is Outcome.Failure -> stepUpFailed(stepUp, result.failure)
        }
    }

    private suspend fun mutate(
        stepUp: SecretDialog.StepUp,
        pending: PendingSignature,
        signature: String,
    ) {
        val cas = currentVersion() ?: return
        val challenge = pending.challenge.challenge
        val action = pending.action
        when (pending.operation) {
            SecretOperation.UPDATE -> mutationResult(
                stepUp,
                repository.update(
                    currentState.path,
                    action.key.orEmpty(),
                    action.value,
                    cas,
                    challenge,
                    signature,
                ),
            )
            SecretOperation.ROLLBACK -> mutationResult(
                stepUp,
                repository.rollback(
                    currentState.path,
                    action.version ?: return,
                    cas,
                    challenge,
                    signature,
                ),
            )
            SecretOperation.DELETE -> mutationResult(
                stepUp,
                repository.delete(
                    currentState.path,
                    cas,
                    action.version ?: return,
                    challenge,
                    signature,
                ),
            )
            SecretOperation.SESSION -> Unit
        }
    }

    private fun cancelled() {
        signing = null
        val stepUp = currentState.dialog as? SecretDialog.StepUp ?: return
        updateState { copy(dialog = stepUp.copy(busy = false)) }
    }

    private fun signingFailed(reason: DeviceSignatureFailure) {
        signing = null
        val stepUp = currentState.dialog as? SecretDialog.StepUp
        val problem = signatureProblem(reason)
        updateState { copy(dialog = stepUp?.copy(busy = false, problem = problem)) }
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
        updateState {
            copy(
                dialog = stepUp.copy(busy = false, problem = problem),
                sessionActive = if (problem == SecretProblem.StepUpRequired) false else sessionActive,
            )
        }
    }

    private fun operationFor(action: SecretAction): SecretOperation = when (action.kind) {
        SecretActionKind.REVEAL -> SecretOperation.SESSION
        SecretActionKind.UPDATE -> SecretOperation.UPDATE
        SecretActionKind.ROLLBACK -> SecretOperation.ROLLBACK
        SecretActionKind.DELETE -> SecretOperation.DELETE
    }

    private fun pathFor(operation: SecretOperation): String? =
        if (operation == SecretOperation.SESSION) null else currentState.path

    private fun keyFor(action: SecretAction): String? =
        if (action.kind == SecretActionKind.UPDATE) action.key else null

    private fun versionFor(action: SecretAction): Long? = when (action.kind) {
        SecretActionKind.ROLLBACK, SecretActionKind.DELETE -> action.version
        else -> null
    }

    private fun currentVersion(): Long? = currentState.metadata?.currentVersion

    private data class PendingSignature(
        val action: SecretAction,
        val operation: SecretOperation,
        val challenge: StepUpChallenge,
    )

    private companion object {
        const val RE_MASK_MILLIS = 30_000L
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(SecretPathViewModel::class)
@Inject
class SecretPathViewModelSource internal constructor(
    private val viewModel: () -> SecretPathViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

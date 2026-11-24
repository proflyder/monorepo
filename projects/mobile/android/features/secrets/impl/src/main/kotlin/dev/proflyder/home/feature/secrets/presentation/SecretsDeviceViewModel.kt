package dev.proflyder.home.feature.secrets.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.secrets.SecretsRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal data class SecretsDeviceUiState(
    val loading: Boolean = true,
    val enrolled: Boolean = false,
    val deviceId: String? = null,
    val code: String = "",
    val keyReady: Boolean = false,
    val rebinding: Boolean = false,
    val busy: Boolean = false,
    val failure: RemoteFailure? = null,
    val problem: SecretProblem? = null,
)

internal sealed interface SecretsDeviceIntent {
    data object Refresh : SecretsDeviceIntent
    data class Code(val value: String) : SecretsDeviceIntent
    data object Bind : SecretsDeviceIntent
    data object Rebind : SecretsDeviceIntent
    data class KeyReady(val material: DeviceKeyMaterial) : SecretsDeviceIntent
    data class KeyFailed(val reason: DeviceSignatureFailure) : SecretsDeviceIntent
}

internal sealed interface SecretsDeviceEvent {
    data class CreateKey(val challenge: String) : SecretsDeviceEvent
    data class ReconcileKey(val serverPublicKey: String?) : SecretsDeviceEvent
    data object Bound : SecretsDeviceEvent
    data class Problem(val problem: SecretProblem) : SecretsDeviceEvent
}

@Inject
internal class SecretsDeviceViewModel(
    private val repository: SecretsRepository,
) : StateViewModel<SecretsDeviceUiState, SecretsDeviceIntent, SecretsDeviceEvent>(SecretsDeviceUiState()) {
    private var material: DeviceKeyMaterial? = null
    private var attestation: String? = null

    init {
        start()
    }

    override fun handle(intent: SecretsDeviceIntent) {
        when (intent) {
            SecretsDeviceIntent.Refresh -> start()
            is SecretsDeviceIntent.Code -> updateState {
                copy(code = intent.value.uppercase().filter { it.isLetterOrDigit() }, problem = null)
            }
            SecretsDeviceIntent.Bind -> bind()
            SecretsDeviceIntent.Rebind -> rebind()
            is SecretsDeviceIntent.KeyReady -> keyReady(intent.material)
            is SecretsDeviceIntent.KeyFailed -> keyFailed(intent.reason)
        }
    }

    private fun start() {
        updateState { copy(loading = true, failure = null, problem = null) }
        launch {
            when (val device = repository.device()) {
                is Outcome.Success -> {
                    updateState {
                        copy(
                            loading = false,
                            enrolled = device.value.enrolled,
                            deviceId = device.value.deviceId,
                        )
                    }
                    when {
                        !device.value.enrolled -> requestKey()
                        device.value.publicKey != null -> emit(
                            SecretsDeviceEvent.ReconcileKey(device.value.publicKey),
                        )
                    }
                }
                is Outcome.Failure -> updateState { copy(loading = false, failure = device.failure) }
            }
        }
    }

    private fun rebind() {
        if (!currentState.enrolled || currentState.rebinding || currentState.busy) return
        material = null
        attestation = null
        updateState {
            copy(
                rebinding = true,
                keyReady = false,
                code = "",
                problem = null,
                failure = null,
            )
        }
        requestKey()
    }

    private fun requestKey() {
        launch {
            when (val challenge = repository.deviceChallenge()) {
                is Outcome.Success -> {
                    attestation = challenge.value.challenge
                    emit(SecretsDeviceEvent.CreateKey(challenge.value.challenge))
                }
                is Outcome.Failure -> {
                    val problem = secretProblem(challenge.failure)
                    updateState { copy(problem = problem, rebinding = false) }
                    emit(SecretsDeviceEvent.Problem(problem))
                }
            }
        }
    }

    private fun bind() {
        val state = currentState
        if (state.busy || state.code.isBlank()) return
        if (state.enrolled && !state.rebinding) return
        val material = material
        val attestation = attestation
        if (material == null || attestation == null) {
            updateState { copy(problem = SecretProblem.DeviceUnavailable) }
            return
        }
        updateState { copy(busy = true, problem = null, failure = null) }
        launch {
            when (
                val result = repository.enroll(state.code, material.publicKey, attestation, material.attestationChain)
            ) {
                is Outcome.Success -> {
                    updateState {
                        copy(
                            busy = false,
                            enrolled = result.value.enrolled,
                            deviceId = result.value.deviceId,
                            code = "",
                            rebinding = false,
                        )
                    }
                    emit(SecretsDeviceEvent.Bound)
                }
                is Outcome.Failure -> updateState {
                    copy(busy = false, problem = secretProblem(result.failure))
                }
            }
        }
    }

    private fun keyReady(material: DeviceKeyMaterial) {
        this.material = material
        updateState { copy(keyReady = true, problem = null) }
    }

    private fun keyFailed(reason: DeviceSignatureFailure) {
        val problem = signatureProblem(reason)
        updateState { copy(keyReady = false, problem = problem) }
        emit(SecretsDeviceEvent.Problem(problem))
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(SecretsDeviceViewModel::class)
@Inject
class SecretsDeviceViewModelSource internal constructor(
    private val viewModel: () -> SecretsDeviceViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}

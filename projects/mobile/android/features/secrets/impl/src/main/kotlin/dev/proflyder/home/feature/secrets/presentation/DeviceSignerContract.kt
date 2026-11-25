package dev.proflyder.home.feature.secrets.presentation

import androidx.fragment.app.FragmentActivity

internal enum class DeviceSignatureFailure {
    UNSUPPORTED,
    KEY_MISSING,
    KEYSTORE,
    AUTHENTICATION,
    KEY_INVALIDATED,
}

internal sealed interface DeviceSignatureResult {
    data class Signed(val signature: String) : DeviceSignatureResult

    data object Cancelled : DeviceSignatureResult

    data class Failed(val reason: DeviceSignatureFailure) : DeviceSignatureResult
}

internal data class DeviceKeyMaterial(
    val publicKey: String,
    val attestationChain: List<String>,
)

internal sealed interface DeviceKeyResult {
    data class Ready(val material: DeviceKeyMaterial) : DeviceKeyResult

    data class Failed(val reason: DeviceSignatureFailure) : DeviceKeyResult
}

internal interface SecretsDeviceSigner {
    fun isSupported(): Boolean

    fun createEnrollmentKey(attestationChallenge: String): DeviceKeyResult

    fun requestSignature(
        activity: FragmentActivity,
        payload: String,
        onResult: (DeviceSignatureResult) -> Unit,
    )

    fun promoteEnrollmentKey()

    fun discardEnrollmentKey()

    fun reconcileEnrollmentKey(serverPublicKey: String?)

    fun cancel()
}

package dev.proflyder.home.feature.secrets.presentation

import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.UserNotAuthenticatedException
import androidx.annotation.RequiresApi
import androidx.fragment.app.FragmentActivity
import dev.proflyder.home.feature.secrets.impl.R
import java.security.Signature
import java.util.concurrent.Executor

internal interface BiometricLauncher {
    fun authenticate(
        crypto: BiometricPrompt.CryptoObject,
        cancellation: CancellationSignal,
        executor: Executor,
        callback: BiometricPrompt.AuthenticationCallback,
    )
}

@RequiresApi(Build.VERSION_CODES.P)
internal class PlatformBiometricLauncher(
    private val prompt: BiometricPrompt,
) : BiometricLauncher {
    override fun authenticate(
        crypto: BiometricPrompt.CryptoObject,
        cancellation: CancellationSignal,
        executor: Executor,
        callback: BiometricPrompt.AuthenticationCallback,
    ) {
        prompt.authenticate(crypto, cancellation, executor, callback)
    }

    companion object {
        fun create(context: Context, activity: FragmentActivity): PlatformBiometricLauncher {
            val builder = BiometricPrompt.Builder(activity)
                .setTitle(context.getString(R.string.secrets_stepup_title))
                .setSubtitle(context.getString(R.string.secrets_stepup_message))
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                builder.setAllowedAuthenticators(
                    BiometricManager.Authenticators.BIOMETRIC_STRONG or
                        BiometricManager.Authenticators.DEVICE_CREDENTIAL,
                )
            }
            return PlatformBiometricLauncher(builder.build())
        }
    }
}

@RequiresApi(Build.VERSION_CODES.P)
internal class BiometricSigning {
    private var pending: ((DeviceSignatureResult) -> Unit)? = null
    private var signal: CancellationSignal? = null

    fun start(request: SigningRequest, onResult: (DeviceSignatureResult) -> Unit) {
        pending = onResult
        signal = request.cancellation
        request.launcher.authenticate(
            BiometricPrompt.CryptoObject(request.signature),
            request.cancellation,
            request.executor,
            Callback(request.payload),
        )
    }

    fun cancel() {
        signal?.cancel()
        signal = null
        pending = null
    }

    private fun finish(result: DeviceSignatureResult) {
        val callback = pending
        pending = null
        signal = null
        callback?.invoke(result)
    }

    private inner class Callback(private val payload: String) : BiometricPrompt.AuthenticationCallback() {
        override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
            val crypto = result.cryptoObject?.signature
            finish(crypto?.let(::sign) ?: DeviceSignatureResult.Failed(DeviceSignatureFailure.KEYSTORE))
        }

        override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
            finish(errorResult(errorCode))
        }

        private fun sign(signature: Signature): DeviceSignatureResult = runCatching {
            signature.update(payload.toByteArray(Charsets.UTF_8))
            signature.sign()
        }.fold(
            onSuccess = { DeviceSignatureResult.Signed(DeviceKeyStore.encode(it)) },
            onFailure = { DeviceSignatureResult.Failed(signFailureOf(it)) },
        )
    }

    private fun errorResult(errorCode: Int): DeviceSignatureResult = when (errorCode) {
        BiometricPrompt.BIOMETRIC_ERROR_CANCELED,
        BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED,
        -> DeviceSignatureResult.Cancelled
        else -> DeviceSignatureResult.Failed(DeviceSignatureFailure.AUTHENTICATION)
    }

    private fun signFailureOf(error: Throwable): DeviceSignatureFailure = when (error) {
        is KeyPermanentlyInvalidatedException -> DeviceSignatureFailure.KEY_INVALIDATED
        is UserNotAuthenticatedException -> DeviceSignatureFailure.AUTHENTICATION
        else -> DeviceSignatureFailure.KEYSTORE
    }
}

internal data class SigningRequest(
    val launcher: BiometricLauncher,
    val signature: Signature,
    val payload: String,
    val cancellation: CancellationSignal,
    val executor: Executor,
)

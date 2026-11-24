@file:Suppress("NoTry")

package dev.proflyder.home.feature.secrets.presentation

import android.annotation.SuppressLint
import android.app.KeyguardManager
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import android.util.Base64
import androidx.annotation.RequiresApi
import androidx.fragment.app.FragmentActivity
import dev.proflyder.home.feature.secrets.impl.R
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.ProviderException
import java.security.Signature
import java.security.SignatureException
import java.security.spec.ECGenParameterSpec

internal enum class DeviceSignatureFailure { UNSUPPORTED, KEY_MISSING, KEYSTORE, AUTHENTICATION }

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

    fun cancel()
}

@SuppressLint("NewApi")
internal class AndroidDeviceSigner(private val context: Context) : SecretsDeviceSigner {
    private var pending: ((DeviceSignatureResult) -> Unit)? = null
    private var signal: CancellationSignal? = null

    override fun isSupported(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && biometrics() && keyguard()

    override fun createEnrollmentKey(attestationChallenge: String): DeviceKeyResult {
        if (!isSupported()) return DeviceKeyResult.Failed(DeviceSignatureFailure.UNSUPPORTED)
        return try {
            val store = keyStore()
            if (store.containsAlias(ALIAS)) store.deleteEntry(ALIAS)
            val pair = pair(attestationChallenge)
            val chain = store.getCertificateChain(ALIAS)?.map { encode(it.encoded) }.orEmpty()
            if (chain.isEmpty()) {
                DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
            } else {
                DeviceKeyResult.Ready(DeviceKeyMaterial(encode(pair.public.encoded), chain))
            }
        } catch (error: GeneralSecurityException) {
            DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
        } catch (error: ProviderException) {
            DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
        } catch (error: IOException) {
            DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
        }
    }

    override fun requestSignature(
        activity: FragmentActivity,
        payload: String,
        onResult: (DeviceSignatureResult) -> Unit,
    ) {
        if (!isSupported()) {
            onResult(DeviceSignatureResult.Failed(DeviceSignatureFailure.UNSUPPORTED))
            return
        }
        val signature = signature()
        if (signature is SignatureResult.Failed) {
            onResult(DeviceSignatureResult.Failed(signature.reason))
            return
        }
        pending = onResult
        val cancellation = CancellationSignal()
        signal = cancellation
        prompt(activity).authenticate(
            cancellation,
            activity.mainExecutor,
            callback((signature as SignatureResult.Ready).signature, payload),
        )
    }

    override fun cancel() {
        signal?.cancel()
        signal = null
        pending = null
    }

    private fun keyStore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

    private fun pair(attestationChallenge: String): KeyPair {
        val pair = strongBoxPair(attestationChallenge)
        return pair ?: regularPair(attestationChallenge)
    }

    private fun strongBoxPair(attestationChallenge: String): KeyPair? = try {
        generate(attestationChallenge, strongBox = true)
    } catch (error: StrongBoxUnavailableException) {
        null
    }

    private fun regularPair(attestationChallenge: String): KeyPair =
        generate(attestationChallenge, strongBox = false)

    private fun generate(attestationChallenge: String, strongBox: Boolean): KeyPair {
        val generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, PROVIDER)
        generator.initialize(spec(attestationChallenge, strongBox))
        return generator.generateKeyPair()
    }

    @Suppress("DEPRECATION")
    private fun spec(attestationChallenge: String, strongBox: Boolean): KeyGenParameterSpec {
        val builder = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_SIGN)
            .setAlgorithmParameterSpec(ECGenParameterSpec(CURVE))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setAttestationChallenge(attestationChallenge.toByteArray(Charsets.UTF_8))
            .setUserAuthenticationRequired(true)
            .setInvalidatedByBiometricEnrollment(true)
        when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> builder.setUserAuthenticationParameters(
                0,
                KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL,
            )
            else -> builder.setUserAuthenticationValidityDurationSeconds(-1)
        }
        if (strongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            builder.setIsStrongBoxBacked(true)
        }
        return builder.build()
    }

    private fun signature(): SignatureResult {
        val key = try {
            keyStore().getKey(ALIAS, null) as? PrivateKey
        } catch (error: GeneralSecurityException) {
            null
        } catch (error: IOException) {
            null
        }
        if (key == null) return SignatureResult.Failed(DeviceSignatureFailure.KEY_MISSING)
        val signature = try {
            Signature.getInstance(SIGNATURE).apply { initSign(key) }
        } catch (error: GeneralSecurityException) {
            return SignatureResult.Failed(DeviceSignatureFailure.KEYSTORE)
        }
        return SignatureResult.Ready(signature)
    }

    @RequiresApi(Build.VERSION_CODES.P)
    private fun prompt(activity: FragmentActivity): BiometricPrompt {
        val builder = BiometricPrompt.Builder(activity)
            .setTitle(context.getString(R.string.secrets_stepup_title))
            .setSubtitle(context.getString(R.string.secrets_stepup_message))
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            builder.setAllowedAuthenticators(
                BiometricManager.Authenticators.BIOMETRIC_STRONG or
                    BiometricManager.Authenticators.DEVICE_CREDENTIAL,
            )
        }
        return builder.build()
    }

    private fun callback(
        signature: Signature,
        payload: String,
    ): BiometricPrompt.AuthenticationCallback =
        object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                val crypto = result.cryptoObject?.signature ?: signature
                val bytes = try {
                    crypto.update(payload.toByteArray(Charsets.UTF_8))
                    crypto.sign()
                } catch (error: SignatureException) {
                    finish(DeviceSignatureResult.Failed(DeviceSignatureFailure.KEYSTORE))
                    return
                }
                finish(DeviceSignatureResult.Signed(encode(bytes)))
            }

            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                val result = when (errorCode) {
                    BiometricPrompt.BIOMETRIC_ERROR_CANCELED,
                    BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED,
                    -> DeviceSignatureResult.Cancelled
                    else -> DeviceSignatureResult.Failed(DeviceSignatureFailure.AUTHENTICATION)
                }
                finish(result)
            }
        }

    private fun finish(result: DeviceSignatureResult) {
        val callback = pending
        pending = null
        signal = null
        callback?.invoke(result)
    }

    private fun biometrics(): Boolean {
        val features = context.packageManager
        return features.hasSystemFeature(PackageManager.FEATURE_FINGERPRINT) ||
            features.hasSystemFeature(FEATURE_FACE) ||
            features.hasSystemFeature(FEATURE_IRIS)
    }

    private fun keyguard(): Boolean {
        val manager = context.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
        return manager?.isDeviceSecure == true
    }

    private fun encode(bytes: ByteArray): String =
        Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)

    private sealed interface SignatureResult {
        data class Ready(val signature: Signature) : SignatureResult

        data class Failed(val reason: DeviceSignatureFailure) : SignatureResult
    }

    private companion object {
        const val PROVIDER = "AndroidKeyStore"
        const val ALIAS = "home_secrets_step_up"
        const val CURVE = "secp256r1"
        const val SIGNATURE = "SHA256withECDSA"
        const val FEATURE_FACE = "android.hardware.face"
        const val FEATURE_IRIS = "android.hardware.iris"
    }
}

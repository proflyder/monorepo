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
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.UserNotAuthenticatedException
import android.util.Base64
import androidx.annotation.RequiresApi
import androidx.core.content.edit
import androidx.fragment.app.FragmentActivity
import dev.proflyder.home.feature.secrets.impl.R
import java.security.InvalidKeyException
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.SignatureException
import java.security.spec.ECGenParameterSpec
import java.util.concurrent.Executor

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

internal interface BiometricLauncher {
    fun authenticate(
        crypto: BiometricPrompt.CryptoObject,
        cancellation: CancellationSignal,
        executor: Executor,
        callback: BiometricPrompt.AuthenticationCallback,
    )
}

@RequiresApi(Build.VERSION_CODES.P)
private class PlatformBiometricLauncher(
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
}

@SuppressLint("NewApi")
internal class AndroidDeviceSigner(
    private val context: Context,
    private val launcher: (FragmentActivity) -> BiometricLauncher = { activity ->
        PlatformBiometricLauncher(prompt(context, activity))
    },
) : SecretsDeviceSigner {
    private var pending: ((DeviceSignatureResult) -> Unit)? = null
    private var signal: CancellationSignal? = null
    private val staging = StagingAliasState(context)

    override fun isSupported(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && biometrics() && keyguard()

    override fun createEnrollmentKey(attestationChallenge: String): DeviceKeyResult {
        if (!isSupported()) return DeviceKeyResult.Failed(DeviceSignatureFailure.UNSUPPORTED)
        val staged = staging.staged()
        if (canReuseStagedKey(staged, staging.stagedChallenge(), attestationChallenge)) {
            val alias = staged ?: return DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
            val publicKey = publicKey(alias)
            val chain = certificateChain(alias)
            if (publicKey != null && chain.isNotEmpty()) {
                sweepStagingAliases(keep = alias)
                return DeviceKeyResult.Ready(DeviceKeyMaterial(publicKey, chain))
            }
        }
        staged?.let(::deleteAlias)
        sweepStagingAliases(keep = activeAlias())
        return runCatching { newEnrollmentKey(attestationChallenge) }
            .getOrElse { DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE) }
    }

    private fun newEnrollmentKey(attestationChallenge: String): DeviceKeyResult {
        val alias = STAGING_PREFIX + java.util.UUID.randomUUID()
        val pair = createPair(alias, attestationChallenge)
            ?: return DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
        val chain = certificateChain(alias)
        if (chain.isEmpty()) {
            deleteAlias(alias)
            return DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
        }
        staging.stage(alias, attestationChallenge)
        return DeviceKeyResult.Ready(DeviceKeyMaterial(encode(pair.public.encoded), chain))
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
        val cancellation = CancellationSignal()
        start(
            launcher = launcher(activity),
            signature = (signature as SignatureResult.Ready).signature,
            payload = payload,
            cancellation = cancellation,
            executor = activity.mainExecutor,
            onResult = onResult,
        )
    }

    internal fun start(
        launcher: BiometricLauncher,
        signature: Signature,
        payload: String,
        cancellation: CancellationSignal,
        executor: Executor,
        onResult: (DeviceSignatureResult) -> Unit,
    ) {
        pending = onResult
        signal = cancellation
        launcher.authenticate(
            BiometricPrompt.CryptoObject(signature),
            cancellation,
            executor,
            callback(payload),
        )
    }

    override fun promoteEnrollmentKey() {
        val staged = staging.staged() ?: return
        val previous = activeAlias()
        preferences().edit(commit = true) { putString(ACTIVE_ALIAS_KEY, staged) }
        staging.clear()
        if (previous != staged) deleteAlias(previous)
        sweepStagingAliases(keep = staged)
    }

    override fun discardEnrollmentKey() {
        val staged = staging.staged() ?: return
        deleteAlias(staged)
        staging.clear()
        sweepStagingAliases(keep = activeAlias())
    }

    override fun reconcileEnrollmentKey(serverPublicKey: String?) {
        val staged = staging.staged()
        if (staged == null) {
            sweepStagingAliases(keep = activeAlias())
            return
        }
        if (samePublicKey(publicKey(staged), serverPublicKey)) {
            promoteEnrollmentKey()
        } else {
            discardEnrollmentKey()
        }
    }

    private fun sweepStagingAliases(keep: String?) {
        val aliases = runCatching { keyStore().aliases() }.getOrNull() ?: return
        while (aliases.hasMoreElements()) {
            val alias = aliases.nextElement()
            if (alias.startsWith(STAGING_PREFIX) && alias != keep) deleteAlias(alias)
        }
    }

    private fun publicKey(alias: String): String? =
        runCatching {
            keyStore().getCertificate(alias)?.publicKey?.encoded?.let(::encode)
        }.getOrNull()

    private fun certificateChain(alias: String): List<String> =
        runCatching {
            keyStore().getCertificateChain(alias)?.map { encode(it.encoded) }.orEmpty()
        }.getOrElse { emptyList() }

    private fun deleteAlias(alias: String) {
        runCatching { keyStore().deleteEntry(alias) }.onFailure { return }
    }

    override fun cancel() {
        signal?.cancel()
        signal = null
        pending = null
    }

    private fun keyStore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

    private fun createPair(alias: String, attestationChallenge: String): KeyPair? {
        val first = runCatching { pair(alias, attestationChallenge, unlockedRequired = true) }
            .onFailure { deleteAlias(alias) }
            .getOrNull()
        return first ?: runCatching { pair(alias, attestationChallenge, unlockedRequired = false) }
            .getOrNull()
    }

    private fun pair(
        alias: String,
        attestationChallenge: String,
        unlockedRequired: Boolean,
    ): KeyPair {
        val pair = strongBoxPair(alias, attestationChallenge, unlockedRequired)
        return pair ?: regularPair(alias, attestationChallenge, unlockedRequired)
    }

    private fun strongBoxPair(
        alias: String,
        attestationChallenge: String,
        unlockedRequired: Boolean,
    ): KeyPair? = runCatching {
        generate(alias, attestationChallenge, strongBox = true, unlockedRequired)
    }.getOrNull()

    private fun regularPair(
        alias: String,
        attestationChallenge: String,
        unlockedRequired: Boolean,
    ): KeyPair = generate(alias, attestationChallenge, strongBox = false, unlockedRequired)

    private fun generate(
        alias: String,
        attestationChallenge: String,
        strongBox: Boolean,
        unlockedRequired: Boolean,
    ): KeyPair {
        val generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, PROVIDER)
        generator.initialize(spec(alias, attestationChallenge, strongBox, unlockedRequired))
        return generator.generateKeyPair()
    }

    @Suppress("DEPRECATION")
    private fun spec(
        alias: String,
        attestationChallenge: String,
        strongBox: Boolean,
        unlockedRequired: Boolean,
    ): KeyGenParameterSpec {
        val builder = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
            .setAlgorithmParameterSpec(ECGenParameterSpec(CURVE))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setAttestationChallenge(attestationChallenge.toByteArray(Charsets.UTF_8))
            .setUserAuthenticationRequired(true)
            .setInvalidatedByBiometricEnrollment(true)
        if (unlockedRequired) builder.setUnlockedDeviceRequired(true)
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
        val key = runCatching {
            keyStore().getKey(activeAlias(), null) as? PrivateKey
        }.getOrNull()
        if (key == null) return SignatureResult.Failed(DeviceSignatureFailure.KEY_MISSING)
        return runCatching {
            Signature.getInstance(SIGNATURE).apply { initSign(key) }
        }.fold(
            onSuccess = { SignatureResult.Ready(it) },
            onFailure = { error ->
                when (error) {
                    is KeyPermanentlyInvalidatedException ->
                        SignatureResult.Failed(DeviceSignatureFailure.KEY_INVALIDATED)
                    else -> SignatureResult.Failed(DeviceSignatureFailure.KEYSTORE)
                }
            },
        )
    }

    private fun callback(payload: String): BiometricPrompt.AuthenticationCallback =
        object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                val crypto = result.cryptoObject?.signature
                if (crypto == null) {
                    finish(DeviceSignatureResult.Failed(DeviceSignatureFailure.KEYSTORE))
                    return
                }
                val signed = runCatching {
                    crypto.update(payload.toByteArray(Charsets.UTF_8))
                    crypto.sign()
                }.fold(
                    onSuccess = { bytes -> DeviceSignatureResult.Signed(encode(bytes)) },
                    onFailure = { error ->
                        when (error) {
                            is KeyPermanentlyInvalidatedException ->
                                DeviceSignatureResult.Failed(DeviceSignatureFailure.KEY_INVALIDATED)
                            is UserNotAuthenticatedException ->
                                DeviceSignatureResult.Failed(DeviceSignatureFailure.AUTHENTICATION)
                            is InvalidKeyException ->
                                DeviceSignatureResult.Failed(DeviceSignatureFailure.KEYSTORE)
                            is SignatureException ->
                                DeviceSignatureResult.Failed(DeviceSignatureFailure.KEYSTORE)
                            else -> DeviceSignatureResult.Failed(DeviceSignatureFailure.KEYSTORE)
                        }
                    },
                )
                finish(signed)
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

    private fun preferences() =
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    private fun activeAlias(): String =
        preferences().getString(ACTIVE_ALIAS_KEY, ALIAS) ?: ALIAS

    private sealed interface SignatureResult {
        data class Ready(val signature: Signature) : SignatureResult

        data class Failed(val reason: DeviceSignatureFailure) : SignatureResult
    }

    private companion object {
        const val PROVIDER = "AndroidKeyStore"
        const val ALIAS = "home_secrets_step_up"
        const val STAGING_PREFIX = "home_secrets_step_up_staging_"
        const val PREFERENCES = "home_secrets_device"
        const val ACTIVE_ALIAS_KEY = "active_alias"
        const val CURVE = "secp256r1"
        const val SIGNATURE = "SHA256withECDSA"
        const val FEATURE_FACE = "android.hardware.face"
        const val FEATURE_IRIS = "android.hardware.iris"

        @RequiresApi(Build.VERSION_CODES.P)
        fun prompt(context: Context, activity: FragmentActivity): BiometricPrompt {
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
    }
}

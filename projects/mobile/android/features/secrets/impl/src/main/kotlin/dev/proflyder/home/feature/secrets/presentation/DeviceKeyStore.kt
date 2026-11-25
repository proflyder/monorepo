package dev.proflyder.home.feature.secrets.presentation

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.annotation.RequiresApi
import androidx.core.content.edit
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.util.UUID

internal sealed interface SigningKey {
    data class Ready(val signature: Signature) : SigningKey

    data class Failed(val reason: DeviceSignatureFailure) : SigningKey
}

internal class DeviceKeyStore(private val context: Context) {
    fun activeAlias(): String = preferences().getString(ACTIVE_ALIAS_KEY, ALIAS) ?: ALIAS

    fun activate(alias: String) {
        preferences().edit(commit = true) { putString(ACTIVE_ALIAS_KEY, alias) }
    }

    fun newStagingAlias(): String = STAGING_PREFIX + UUID.randomUUID()

    fun publicKey(alias: String): String? =
        runCatching { keyStore().getCertificate(alias)?.publicKey?.encoded?.let(::encode) }.getOrNull()

    fun certificateChain(alias: String): List<String> =
        runCatching { keyStore().getCertificateChain(alias)?.map { encode(it.encoded) }.orEmpty() }
            .getOrElse { emptyList() }

    fun deleteAlias(alias: String) {
        runCatching { keyStore().deleteEntry(alias) }
    }

    fun sweepStagingAliases(keep: String?) {
        val aliases = runCatching { keyStore().aliases().toList() }.getOrNull() ?: return
        aliases.filter { it.startsWith(STAGING_PREFIX) && it != keep }.forEach(::deleteAlias)
    }

    fun signingKey(): SigningKey {
        val key = runCatching { keyStore().getKey(activeAlias(), null) as? PrivateKey }.getOrNull()
            ?: return SigningKey.Failed(DeviceSignatureFailure.KEY_MISSING)
        return runCatching { Signature.getInstance(SIGNATURE).apply { initSign(key) } }.fold(
            onSuccess = { SigningKey.Ready(it) },
            onFailure = { SigningKey.Failed(failureOf(it)) },
        )
    }

    @RequiresApi(Build.VERSION_CODES.P)
    fun createPair(alias: String, attestationChallenge: String): KeyPair? {
        val locked = runCatching { pair(alias, attestationChallenge, unlockedRequired = true) }
            .onFailure { deleteAlias(alias) }
            .getOrNull()
        return locked ?: runCatching { pair(alias, attestationChallenge, unlockedRequired = false) }.getOrNull()
    }

    @RequiresApi(Build.VERSION_CODES.P)
    private fun pair(alias: String, attestationChallenge: String, unlockedRequired: Boolean): KeyPair {
        val spec = KeySpec(alias, attestationChallenge, unlockedRequired)
        return runCatching { generate(spec, strongBox = true) }.getOrNull() ?: generate(spec, strongBox = false)
    }

    @RequiresApi(Build.VERSION_CODES.P)
    private fun generate(spec: KeySpec, strongBox: Boolean): KeyPair {
        val generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, PROVIDER)
        generator.initialize(parameters(spec, strongBox))
        return generator.generateKeyPair()
    }

    @RequiresApi(Build.VERSION_CODES.P)
    private fun parameters(spec: KeySpec, strongBox: Boolean): KeyGenParameterSpec {
        val builder = KeyGenParameterSpec.Builder(spec.alias, KeyProperties.PURPOSE_SIGN)
            .setAlgorithmParameterSpec(ECGenParameterSpec(CURVE))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setAttestationChallenge(spec.attestationChallenge.toByteArray(Charsets.UTF_8))
            .setUserAuthenticationRequired(true)
            .setInvalidatedByBiometricEnrollment(true)
            .setUnlockedDeviceRequired(spec.unlockedRequired)
            .setIsStrongBoxBacked(strongBox)
        requireEveryUse(builder)
        return builder.build()
    }

    private fun requireEveryUse(builder: KeyGenParameterSpec.Builder) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            builder.setUserAuthenticationParameters(
                0,
                KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL,
            )
            return
        }
        requireEveryUseLegacy(builder)
    }

    @Suppress("DEPRECATION")
    private fun requireEveryUseLegacy(builder: KeyGenParameterSpec.Builder) {
        builder.setUserAuthenticationValidityDurationSeconds(-1)
    }

    private fun failureOf(error: Throwable): DeviceSignatureFailure = when (error) {
        is KeyPermanentlyInvalidatedException -> DeviceSignatureFailure.KEY_INVALIDATED
        else -> DeviceSignatureFailure.KEYSTORE
    }

    private fun keyStore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

    private fun preferences() = context.deviceKeyPreferences()

    private data class KeySpec(val alias: String, val attestationChallenge: String, val unlockedRequired: Boolean)

    companion object {
        private const val PROVIDER = "AndroidKeyStore"
        private const val ALIAS = "home_secrets_step_up"
        private const val STAGING_PREFIX = "home_secrets_step_up_staging_"
        private const val ACTIVE_ALIAS_KEY = "active_alias"
        private const val CURVE = "secp256r1"
        private const val SIGNATURE = "SHA256withECDSA"

        fun encode(bytes: ByteArray): String =
            Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
    }
}

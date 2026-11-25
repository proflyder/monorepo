package dev.proflyder.home.feature.secrets.presentation

import android.app.KeyguardManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.CancellationSignal
import androidx.annotation.RequiresApi
import androidx.fragment.app.FragmentActivity

internal fun deviceSigner(context: Context): SecretsDeviceSigner =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) AndroidDeviceSigner(context) else UnsupportedDeviceSigner

internal object UnsupportedDeviceSigner : SecretsDeviceSigner {
    override fun isSupported(): Boolean = false

    override fun createEnrollmentKey(attestationChallenge: String): DeviceKeyResult =
        DeviceKeyResult.Failed(DeviceSignatureFailure.UNSUPPORTED)

    override fun requestSignature(
        activity: FragmentActivity,
        payload: String,
        onResult: (DeviceSignatureResult) -> Unit,
    ) = onResult(DeviceSignatureResult.Failed(DeviceSignatureFailure.UNSUPPORTED))

    override fun promoteEnrollmentKey() = Unit

    override fun discardEnrollmentKey() = Unit

    override fun reconcileEnrollmentKey(serverPublicKey: String?) = Unit

    override fun cancel() = Unit
}

@RequiresApi(Build.VERSION_CODES.P)
internal class AndroidDeviceSigner(
    private val context: Context,
    private val launcher: (FragmentActivity) -> BiometricLauncher = { activity ->
        PlatformBiometricLauncher.create(context, activity)
    },
) : SecretsDeviceSigner {
    private val keys = DeviceKeyStore(context)
    private val staging = StagingAliasState(context)
    private val signing = BiometricSigning()

    override fun isSupported(): Boolean = biometrics() && keyguard()

    override fun createEnrollmentKey(attestationChallenge: String): DeviceKeyResult {
        if (!isSupported()) return DeviceKeyResult.Failed(DeviceSignatureFailure.UNSUPPORTED)
        val staged = staging.staged()
        if (staged != null && canReuseStagedKey(staged, staging.stagedChallenge(), attestationChallenge)) {
            reusableMaterial(staged)?.let { return DeviceKeyResult.Ready(it) }
        }
        staged?.let(keys::deleteAlias)
        keys.sweepStagingAliases(keep = keys.activeAlias())
        return runCatching { newEnrollmentKey(attestationChallenge) }
            .getOrElse { DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE) }
    }

    private fun reusableMaterial(alias: String): DeviceKeyMaterial? {
        val publicKey = keys.publicKey(alias) ?: return null
        val chain = keys.certificateChain(alias).takeIf { it.isNotEmpty() } ?: return null
        keys.sweepStagingAliases(keep = alias)
        return DeviceKeyMaterial(publicKey, chain)
    }

    private fun newEnrollmentKey(attestationChallenge: String): DeviceKeyResult {
        val alias = keys.newStagingAlias()
        val pair = keys.createPair(alias, attestationChallenge)
            ?: return DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
        val chain = keys.certificateChain(alias)
        if (chain.isEmpty()) {
            keys.deleteAlias(alias)
            return DeviceKeyResult.Failed(DeviceSignatureFailure.KEYSTORE)
        }
        staging.stage(alias, attestationChallenge)
        return DeviceKeyResult.Ready(DeviceKeyMaterial(DeviceKeyStore.encode(pair.public.encoded), chain))
    }

    override fun requestSignature(
        activity: FragmentActivity,
        payload: String,
        onResult: (DeviceSignatureResult) -> Unit,
    ) {
        if (!isSupported()) return onResult(DeviceSignatureResult.Failed(DeviceSignatureFailure.UNSUPPORTED))
        when (val key = keys.signingKey()) {
            is SigningKey.Failed -> onResult(DeviceSignatureResult.Failed(key.reason))
            is SigningKey.Ready -> signing.start(
                SigningRequest(launcher(activity), key.signature, payload, CancellationSignal(), activity.mainExecutor),
                onResult,
            )
        }
    }

    override fun promoteEnrollmentKey() {
        val staged = staging.staged() ?: return
        val previous = keys.activeAlias()
        keys.activate(staged)
        staging.clear()
        if (previous != staged) keys.deleteAlias(previous)
        keys.sweepStagingAliases(keep = staged)
    }

    override fun discardEnrollmentKey() {
        val staged = staging.staged() ?: return
        keys.deleteAlias(staged)
        staging.clear()
        keys.sweepStagingAliases(keep = keys.activeAlias())
    }

    override fun reconcileEnrollmentKey(serverPublicKey: String?) {
        val staged = staging.staged() ?: return keys.sweepStagingAliases(keep = keys.activeAlias())
        if (samePublicKey(keys.publicKey(staged), serverPublicKey)) promoteEnrollmentKey() else discardEnrollmentKey()
    }

    override fun cancel() = signing.cancel()

    private fun biometrics(): Boolean {
        val features = context.packageManager
        return BIOMETRIC_FEATURES.any(features::hasSystemFeature)
    }

    private fun keyguard(): Boolean {
        val manager = context.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
        return manager?.isDeviceSecure == true
    }

    private companion object {
        val BIOMETRIC_FEATURES = listOf(
            PackageManager.FEATURE_FINGERPRINT,
            "android.hardware.face",
            "android.hardware.iris",
        )
    }
}

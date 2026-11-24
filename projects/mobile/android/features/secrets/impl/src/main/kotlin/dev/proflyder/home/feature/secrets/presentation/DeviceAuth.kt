package dev.proflyder.home.feature.secrets.presentation

import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import androidx.annotation.RequiresApi
import androidx.fragment.app.FragmentActivity
import dev.proflyder.home.feature.secrets.impl.R

internal class DeviceAuth(private val activity: FragmentActivity) {
    private var pending: ((Boolean) -> Unit)? = null
    private var signal: CancellationSignal? = null

    fun authenticate(credentials: (Intent) -> Unit, result: (Boolean) -> Unit) {
        pending = result
        when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && biometricAvailable() -> biometric(credentials)
            else -> credentialsOrFail(credentials)
        }
    }

    fun cancel() {
        signal?.cancel()
        signal = null
        pending = null
    }

    private fun finish(granted: Boolean) {
        val callback = pending
        pending = null
        signal = null
        callback?.invoke(granted)
    }

    @Suppress("DEPRECATION")
    private fun credentialsOrFail(credentials: (Intent) -> Unit) {
        val keyguard = activity.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
        val intent = keyguard?.takeIf { it.isDeviceSecure }?.createConfirmDeviceCredentialIntent(
            activity.getString(R.string.secrets_biometric_title),
            activity.getString(R.string.secrets_biometric_subtitle),
        )
        if (intent == null) finish(false) else credentials(intent)
    }

    private fun biometricAvailable(): Boolean {
        val features = activity.packageManager
        val biometric = features.hasSystemFeature(PackageManager.FEATURE_FINGERPRINT) ||
            features.hasSystemFeature(FEATURE_FACE) ||
            features.hasSystemFeature(FEATURE_IRIS)
        val keyguard = activity.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
        return biometric && keyguard?.isDeviceSecure == true
    }

    @Suppress("DEPRECATION")
    @RequiresApi(Build.VERSION_CODES.P)
    private fun biometric(credentials: (Intent) -> Unit) {
        val builder = BiometricPrompt.Builder(activity)
            .setTitle(activity.getString(R.string.secrets_biometric_title))
            .setSubtitle(activity.getString(R.string.secrets_biometric_subtitle))
        when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> builder.setAllowedAuthenticators(
                BiometricManager.Authenticators.BIOMETRIC_STRONG or
                    BiometricManager.Authenticators.DEVICE_CREDENTIAL,
            )
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q -> builder.setDeviceCredentialAllowed(true)
        }
        val cancellation = CancellationSignal()
        signal = cancellation
        builder.build().authenticate(
            cancellation,
            activity.mainExecutor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    finish(true)
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    when (errorCode) {
                        BiometricPrompt.BIOMETRIC_ERROR_NO_BIOMETRICS,
                        BiometricPrompt.BIOMETRIC_ERROR_HW_NOT_PRESENT,
                        -> credentialsOrFail(credentials)
                        else -> finish(false)
                    }
                }
            },
        )
    }
}

private const val FEATURE_FACE = "android.hardware.face"
private const val FEATURE_IRIS = "android.hardware.iris"

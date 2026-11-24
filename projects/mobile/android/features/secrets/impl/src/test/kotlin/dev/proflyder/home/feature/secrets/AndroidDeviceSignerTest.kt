package dev.proflyder.home.feature.secrets

import android.hardware.biometrics.BiometricPrompt
import android.os.CancellationSignal
import dev.proflyder.home.feature.secrets.presentation.AndroidDeviceSigner
import dev.proflyder.home.feature.secrets.presentation.BiometricLauncher
import java.security.Signature
import java.util.concurrent.Executor
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AndroidDeviceSignerTest {
    private class FakeLauncher : BiometricLauncher {
        var crypto: BiometricPrompt.CryptoObject? = null

        override fun authenticate(
            crypto: BiometricPrompt.CryptoObject,
            cancellation: CancellationSignal,
            executor: Executor,
            callback: BiometricPrompt.AuthenticationCallback,
        ) {
            this.crypto = crypto
        }
    }

    @Test
    fun aSignatureRequestPassesACryptoObjectAndNotAPlainSignature() {
        val signer = AndroidDeviceSigner(RuntimeEnvironment.getApplication()) { FakeLauncher() }
        val launcher = FakeLauncher()
        val signature = Signature.getInstance("SHA256withECDSA")

        signer.start(
            launcher = launcher,
            signature = signature,
            payload = "payload",
            cancellation = CancellationSignal(),
            executor = Executor { command -> command.run() },
            onResult = {},
        )

        assertSame(signature, launcher.crypto?.signature)
    }

    @Test
    fun anUnsupportedPlatformDoesNotRequestASignature() {
        val signer = AndroidDeviceSigner(RuntimeEnvironment.getApplication()) { FakeLauncher() }
        var result: dev.proflyder.home.feature.secrets.presentation.DeviceSignatureResult? = null

        val activity = Robolectric.buildActivity(SecretsTestActivity::class.java).setup().get()
        signer.requestSignature(
            activity = activity,
            payload = "payload",
            onResult = { result = it },
        )

        assertTrue(result is dev.proflyder.home.feature.secrets.presentation.DeviceSignatureResult.Failed)
    }
}

package dev.proflyder.home.feature.vpn.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.VpnOcservSettings
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnWireGuardSettings
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnProtocol
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.VpnSettings
import dev.proflyder.home.feature.vpn.impl.R
import dev.proflyder.home.feature.vpn.impl.databinding.FragmentVpnBinding
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

internal object VpnSamples {
    val profile = VpnProfile(
        deviceId = "device-1",
        deviceName = "Pixel",
        wireguard = VpnWireGuardSettings(
            address = "10.77.2.2/32",
            dns = listOf("192.0.2.1"),
            mtu = 1380,
            serverPublicKey = "server-key",
            endpoint = "vpn.example.com:51820",
            allowedIps = listOf("0.0.0.0/0"),
            splitAllowedIps = listOf("192.0.2.0/24"),
            persistentKeepalive = 25,
            lanEndpoint = "192.0.2.29:51820",
        ),
        ocserv = VpnOcservSettings("https://vpn.example.com", "full", "lan", listOf("certificate", "password+totp")),
        createdUnix = 1_795_100_000,
        updatedUnix = 1_795_100_100,
    )
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class VpnScreenshotTest {
    private val now = 1_795_200_000_000L

    private val connected = VpnUiState(
        connection = VpnConnection.Connected(VpnProtocol.WIREGUARD, now - 754_000, 12_582_912, 1_048_576),
        profile = VpnProfileState.Ready(VpnSamples.profile),
        settings = VpnSettings(route = VpnRoute.HOME, autoConnect = true, certificateName = "owner-phone"),
        atHome = false,
        externalInstalled = true,
    )

    @Test
    @Config(qualifiers = "notnight")
    fun connectedLight() {
        val binding = render(connected)
        assertEquals(binding.root.context.getString(R.string.vpn_disconnect), binding.vpnToggle.text.toString())
        assertTrue(binding.vpnStatusTraffic.visibility == View.VISIBLE)
        assertTrue(binding.vpnAutoRow.switch.isChecked)
        save(binding.root, "vpn-light.png")
    }

    @Test
    @Config(qualifiers = "night")
    fun connectedDark() = save(render(connected).root, "vpn-dark.png")

    @Test
    @Config(qualifiers = "night")
    fun fallbackAndOfflineProfileDark() {
        val state = VpnUiState(
            connection = VpnConnection.FallbackRequired,
            profile = VpnProfileState.Failed(dev.proflyder.home.feature.vpn.VpnProfileProblem.OFFLINE, VpnSamples.profile),
            atHome = null,
        )
        val binding = render(state)
        assertTrue(binding.vpnExternal.visibility == View.VISIBLE)
        assertFalse(binding.vpnStatusTraffic.visibility == View.VISIBLE)
        save(binding.root, "vpn-fallback-dark.png")
    }

    @Test
    @Config(qualifiers = "night")
    fun wireGuardSilentAtHomeOffersNoFallbackDark() {
        val state = VpnUiState(
            connection = VpnConnection.NotNeededAtHome,
            profile = VpnProfileState.Ready(VpnSamples.profile),
            atHome = true,
            externalInstalled = true,
        )
        val binding = render(state)
        val context = binding.root.context
        assertFalse(binding.vpnExternal.visibility == View.VISIBLE)
        assertEquals(context.getString(R.string.vpn_state_home), binding.vpnStatusRow.title.toString())
        assertEquals(context.getString(R.string.vpn_connect), binding.vpnToggle.text.toString())
        save(binding.root, "vpn-home-dark.png")
    }

    private fun render(state: VpnUiState): FragmentVpnBinding {
        val context = themed()
        val binding = FragmentVpnBinding.inflate(LayoutInflater.from(context))
        binding.root.setBackgroundColor(MaterialColors.getColor(context, com.google.android.material.R.attr.colorSurface, 0))
        VpnScreenBinder.labels(binding)
        VpnScreenBinder.render(binding, state, now)
        return binding
    }

    private fun themed(): Context {
        val base = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        return ContextThemeWrapper(base, AppTheme.CURRENT.overlayRes)
    }

    private fun save(root: View, name: String) {
        val metrics = root.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 1100
    }
}

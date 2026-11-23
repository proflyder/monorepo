package dev.proflyder.home.feature.vpn.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.feature.vpn.VpnSamples
import dev.proflyder.home.feature.vpn.VpnTraffic
import dev.proflyder.home.feature.vpn.VpnTransportInfo
import android.widget.Button
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class VpnScreenshotTest {
    private val now = 1_795_200_000_000L

    private val intents = mutableListOf<VpnIntent>()

    private val wireGuard = VpnTransportInfo(VpnProtocolId("wireguard"), R.string.vpn_protocol_wireguard, external = false)

    private val openConnect = VpnTransportInfo(VpnProtocolId("ocserv"), R.string.vpn_protocol_openconnect, external = true)

    private val connected = VpnUiState(
        connection = VpnConnection.Connected(wireGuard, now - 754_000, verified = true),
        traffic = VpnTraffic(12_582_912, 1_048_576),
        nowMillis = now,
        profile = VpnProfileState.Ready(VpnSamples.profile),
        settings = VpnSettings(route = VpnRoute.HOME, autoConnect = true, certificateName = "owner-phone"),
        atHome = false,
        fallbackReady = true,
    )

    @Test
    @Config(qualifiers = "notnight")
    fun connectedLight() {
        val binding = render(connected)
        assertEquals(binding.root.context.getString(R.string.vpn_disconnect), binding.vpnToggle.text.toString())
        assertTrue(binding.vpnStatusTraffic.visibility == View.VISIBLE)
        assertTrue(binding.vpnAutoRow.switch.isChecked)
        assertTrue(binding.vpnOpenRow.switch.isChecked)
        save(binding.root, "vpn-light.png")
    }

    @Test
    @Config(qualifiers = "notnight")
    fun switchesSendIntentsAndShowOnlyTheRenderedState() {
        val binding = render(connected)
        binding.vpnAutoRow.switch.performClick()
        assertEquals(listOf<VpnIntent>(VpnIntent.SetAutoConnect(false)), intents)
        assertTrue(binding.vpnAutoRow.switch.isChecked)
        binding.vpnToggle.performClick()
        assertEquals(VpnIntent.ToggleConnection, intents.last())
        binding.vpnRoute.getChildAt(0).performClick()
        assertEquals(VpnIntent.SetRoute(VpnRoute.ALL), intents.last())
        assertEquals(VpnRoute.HOME.ordinal, binding.vpnRoute.selectedIndex)
    }

    @Test
    @Config(qualifiers = "night")
    fun externalTunnelIsShownAsUnverifiedDark() {
        val state = connected.copy(connection = VpnConnection.Connected(openConnect, now - 60_000, verified = false), traffic = null)
        val binding = render(state)
        val context = binding.root.context
        val label = context.getString(R.string.vpn_protocol_openconnect)
        assertEquals(context.getString(R.string.vpn_detail_connected_unverified, label, "01:00"), binding.vpnStatusRow.subtitle.toString())
        assertFalse(binding.vpnStatusTraffic.visibility == View.VISIBLE)
        save(binding.root, "vpn-external-dark.png")
    }

    @Test
    @Config(qualifiers = "night")
    fun connectedDark() = save(render(connected).root, "vpn-dark.png")

    @Test
    @Config(qualifiers = "night")
    fun fallbackAndOfflineProfileDark() {
        val state = VpnUiState(
            connection = VpnConnection.FallbackRequired(listOf(openConnect)),
            profile = VpnProfileState.Failed(dev.proflyder.home.feature.vpn.VpnProfileProblem.OFFLINE, VpnSamples.profile),
            atHome = null,
        )
        val binding = render(state)
        assertTrue(binding.vpnFallbackOptions.visibility == View.VISIBLE)
        val option = binding.vpnFallbackOptions.getChildAt(0) as Button
        assertEquals(binding.root.context.getString(R.string.vpn_open_via, "OpenConnect"), option.text.toString())
        option.performClick()
        assertEquals(VpnIntent.ConnectVia(VpnProtocolId("ocserv")), intents.last())
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
            fallbackReady = true,
        )
        val binding = render(state)
        val context = binding.root.context
        assertFalse(binding.vpnFallbackOptions.visibility == View.VISIBLE)
        assertEquals(context.getString(R.string.vpn_state_home), binding.vpnStatusRow.title.toString())
        assertEquals(context.getString(R.string.vpn_connect), binding.vpnToggle.text.toString())
        save(binding.root, "vpn-home-dark.png")
    }

    private fun render(state: VpnUiState): FragmentVpnBinding {
        val context = themed()
        val binding = FragmentVpnBinding.inflate(LayoutInflater.from(context))
        binding.root.setBackgroundColor(MaterialColors.getColor(context, com.google.android.material.R.attr.colorSurface, 0))
        val screen = VpnScreen(binding) { intents += it }
        screen.renderStatus(VpnStatus(state.connection, state.traffic, state.nowMillis, state.busy))
        screen.renderRoute(state.settings, state.atHome)
        screen.renderAuth(state.settings, state.fallbackReady)
        screen.renderProfile(state.profile, state.busy)
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

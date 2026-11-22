package dev.proflyder.home.feature.vpn.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import android.widget.LinearLayout
import androidx.appcompat.view.ContextThemeWrapper
import androidx.appcompat.widget.Toolbar
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.button.MaterialButton
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnProtocol
import dev.proflyder.home.feature.vpn.impl.R
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class VpnToolbarScreenshotTest {
    @Test
    @Config(qualifiers = "notnight")
    fun offLight() = shoot(VpnConnection.Disconnected, VpnToggleLook.OFF, R.string.vpn_toggle_off, "vpn-toolbar-off.png")

    @Test
    @Config(qualifiers = "notnight")
    fun connectingLight() = shoot(
        VpnConnection.Connecting(VpnProtocol.WIREGUARD),
        VpnToggleLook.CONNECTING,
        R.string.vpn_toggle_connecting,
        "vpn-toolbar-connecting.png",
    )

    @Test
    @Config(qualifiers = "notnight")
    fun onLight() = shoot(
        VpnConnection.Connected(VpnProtocol.WIREGUARD, 1),
        VpnToggleLook.ON,
        R.string.vpn_toggle_on,
        "vpn-toolbar-on.png",
    )

    @Test
    @Config(qualifiers = "night")
    fun connectingDark() = shoot(
        VpnConnection.Connecting(VpnProtocol.WIREGUARD),
        VpnToggleLook.CONNECTING,
        R.string.vpn_toggle_connecting,
        "vpn-toolbar-connecting-dark.png",
    )

    private fun shoot(connection: VpnConnection, look: VpnToggleLook, description: Int, name: String) {
        val context = themed()
        val button = LayoutInflater.from(context).inflate(R.layout.vpn_toolbar_toggle, null) as MaterialButton
        VpnToolbarToggle.render(button, connection)
        assertEquals(look, VpnToolbarToggle.look(connection))
        assertEquals(context.getString(description), button.contentDescription.toString())
        save(column(context, button, look == VpnToggleLook.CONNECTING), name)
    }

    private fun column(context: Context, button: MaterialButton, connecting: Boolean): View {
        val toolbar = MaterialToolbar(context)
        toolbar.title = TITLE
        toolbar.addView(button, Toolbar.LayoutParams(WRAP, WRAP, android.view.Gravity.END))
        val banner = LayoutInflater.from(context).inflate(R.layout.vpn_route_banner, null)
        banner.visibility = if (connecting) View.VISIBLE else View.GONE
        val root = LinearLayout(context)
        root.orientation = LinearLayout.VERTICAL
        root.setBackgroundColor(MaterialColors.getColor(context, com.google.android.material.R.attr.colorSurface, 0))
        root.addView(toolbar, LinearLayout.LayoutParams(MATCH, toolbarHeight(context)))
        root.addView(banner, LinearLayout.LayoutParams(MATCH, WRAP))
        return root
    }

    private fun toolbarHeight(context: Context): Int = (TOOLBAR_DP * context.resources.displayMetrics.density).toInt()

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
        const val TITLE = "Управление"
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 120
        const val TOOLBAR_DP = 64
        const val WRAP = LinearLayout.LayoutParams.WRAP_CONTENT
        const val MATCH = LinearLayout.LayoutParams.MATCH_PARENT
    }
}

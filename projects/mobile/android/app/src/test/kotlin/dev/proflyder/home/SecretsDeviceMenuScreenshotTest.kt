package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import android.widget.FrameLayout
import android.widget.TextView
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.AppChromeState
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.ToolbarAction
import dev.proflyder.home.core.ui.R as UiR
import dev.proflyder.home.databinding.ActivityMainBinding
import dev.proflyder.home.feature.secrets.impl.R as SecretsR
import java.io.File
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SecretsDeviceMenuScreenshotTest {
    @Test
    fun dark() = shoot("secrets-device-menu-dark.png")

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-notnight-xxhdpi")
    fun light() = shoot("secrets-device-menu-light.png")

    private fun shoot(name: String) {
        val theme = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            R.style.Theme_Home,
        )
        val binding = ActivityMainBinding.inflate(LayoutInflater.from(theme))
        val toolbar = ToolbarChrome(binding, FakeNavigator(), emptySet()) {}
        val chrome = AppChromeState(
            tabId = "secrets",
            titleRes = SecretsR.string.secrets_title,
            canGoBack = true,
            overlay = true,
            screenActions = listOf(DeviceAction()),
        )

        toolbar.render(chrome, ownerAccess())
        binding.toolbar.menu.performIdentifierAction(R.id.action_menu, 0)
        shadowOf(android.os.Looper.getMainLooper()).idle()

        val menu = requireNotNull(toolbar.contentView) { "the device menu must be open" }
        assertTrue(toolbar.visibleActions.isEmpty())
        assertNotNull(menu.findViewById<TextView>(UiR.id.ds_popup_menu_label))
        assertEquals(
            theme.getString(SecretsR.string.secrets_device_menu),
            (menu as android.view.ViewGroup).getChildAt(0)
                .findViewById<TextView>(UiR.id.ds_popup_menu_label)
                .text.toString(),
        )

        val root = FrameLayout(theme).apply {
            setBackgroundColor(MaterialColors.getColor(this, com.google.android.material.R.attr.colorSurface))
        }
        root.addView(binding.root)
        val bitmap = render(root, menu, binding.toolbar.findViewById(R.id.action_menu))
        save(bitmap, name)
        assertTrue(bitmap.width > 0)
        toolbar.dismiss()
    }

    private fun ownerAccess(): AppAccessState = AppAccessState.Ready(
        AppAccess("Owner", AppModule.entries.toSet(), owner = true),
    )

    private fun render(view: View, menu: View, anchor: View): Bitmap {
        val metrics = view.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        repeat(2) {
            view.measure(exactly(width), exactly(height))
            view.layout(0, 0, width, height)
        }
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        view.draw(canvas)
        menu.measure(unspecified(), unspecified())
        menu.layout(0, 0, menu.measuredWidth, menu.measuredHeight)
        canvas.save()
        canvas.translate(
            (width - menu.measuredWidth - (8 * metrics.density).toInt()).toFloat(),
            (anchor.bottom + (4 * metrics.density).toInt()).toFloat(),
        )
        menu.draw(canvas)
        canvas.restore()
        return bitmap
    }

    private fun save(bitmap: Bitmap, name: String) {
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private fun unspecified(): Int = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED)

    private class DeviceAction : ToolbarAction {
        override val itemId: Int = SecretsR.id.secrets_device_action
        override val order: Int = 0
        override val titleRes: Int = SecretsR.string.secrets_device_menu
        override val iconRes: Int = SecretsR.drawable.secrets_ic_device

        override fun perform(navigator: Navigator) = Unit
    }

    private class FakeNavigator : Navigator {
        override val chrome: StateFlow<AppChromeState> = MutableStateFlow(AppChromeState("secrets", 0))

        override fun selectTab(featureId: String) = Unit

        override fun navigate(destination: Destination) = Unit

        override fun openRoute(route: String): Boolean = false

        override fun back(): Boolean = false
    }

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
    }
}

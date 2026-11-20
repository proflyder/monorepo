package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.connection.ConnectionDestination
import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class KeyScreensScreenshotTest {
    @Test
    fun emeraldDark() = shootAll(AppTheme.CURRENT, "dark")

    @Test
    @Config(qualifiers = LIGHT)
    fun emeraldLight() = shootAll(AppTheme.CURRENT, "light")

    @Test
    fun systemDark() = shootAll(AppTheme.SYSTEM, "dark")

    @Test
    @Config(qualifiers = LIGHT)
    fun systemLight() = shootAll(AppTheme.SYSTEM, "light")

    @Test
    fun neonDark() = shootAll(AppTheme.NEON, "dark")

    @Test
    @Config(qualifiers = LIGHT)
    fun neonLight() = shootAll(AppTheme.NEON, "light")

    private fun shootAll(theme: AppTheme, mode: String) {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        app.graph.appearance.selectTheme(theme)
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        TABS.forEach { (name, tab) -> shootTab(activity, tab, "screen-${theme.key}-$mode-$name.png") }
        destinations().forEach { (name, destination) ->
            shootDestination(activity, destination, "screen-${theme.key}-$mode-$name.png")
        }
    }

    private fun shootTab(activity: MainActivity, tab: String, name: String) {
        activity.router.selectTab(tab)
        idle(activity)
        shoot(activity, name)
    }

    private fun shootDestination(activity: MainActivity, destination: Destination, name: String) {
        activity.router.navigate(destination)
        idle(activity)
        shoot(activity, name)
        activity.router.back()
        idle(activity)
    }

    private fun destinations(): List<Pair<String, Destination>> = listOf(
        "connection" to ConnectionDestination,
        "alarm-editor" to AlarmEditorDestination(),
    )

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun shoot(activity: MainActivity, name: String) {
        val bitmap = render(activity.window.decorView)
        assertTrue("$name is empty", bitmap.width > 0)
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun render(root: View): Bitmap {
        val metrics = root.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        return bitmap
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val LIGHT = "w411dp-h891dp-notnight-xxhdpi"
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val PNG_QUALITY = 100
        val TABS = listOf(
            "home" to "control",
            "scenarios" to "scenarios",
            "time" to "time",
            "chat" to "chat",
            "system" to "system",
        )
    }
}

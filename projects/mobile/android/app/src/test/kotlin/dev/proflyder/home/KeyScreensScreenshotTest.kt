package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import android.view.ViewGroup
import com.google.android.material.button.MaterialButton
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.theme.AppTheme
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
import dev.proflyder.home.feature.alarms.impl.R as AlarmsR
import kotlinx.coroutines.runBlocking

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
        runBlocking { app.graph.appearance.selectTheme(theme) }
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        TABS.forEach { (name, tab) -> shootTab(activity, tab, "screen-${theme.key}-$mode-$name.png") }
        destinations().forEach { (name, destination) ->
            shootDestination(activity, destination, "screen-${theme.key}-$mode-$name.png")
        }
        shootAlarmEditor(activity, "screen-${theme.key}-$mode-alarm-editor.png")
    }

    private fun shootTab(activity: MainActivity, tab: String, name: String) {
        activity.router.selectTab(tab)
        idle(activity)
        shoot(activity, name)
        assertOneFilledButton(activity, name)
    }

    private fun assertOneFilledButton(activity: MainActivity, name: String) {
        val primary = MaterialColors.getColor(activity, androidx.appcompat.R.attr.colorPrimary, 0)
        val filled = buttons(activity.findViewById(R.id.fragment_container))
            .filter { it.isShown && !it.text.isNullOrEmpty() && it.backgroundTintList?.defaultColor == primary }
        assertTrue("$name shows ${filled.size} filled buttons", filled.size <= 1)
    }

    private fun buttons(view: View): List<MaterialButton> = when (view) {
        is MaterialButton -> listOf(view)
        is ViewGroup -> (0 until view.childCount).flatMap { buttons(view.getChildAt(it)) }
        else -> emptyList()
    }

    private fun shootDestination(activity: MainActivity, destination: Destination, name: String) {
        activity.router.navigate(destination)
        idle(activity)
        shoot(activity, name)
        activity.router.back()
        idle(activity)
    }

    private fun shootAlarmEditor(activity: MainActivity, name: String) {
        activity.router.selectTab("alarms")
        idle(activity)
        activity.findViewById<View>(AlarmsR.id.alarms_add).performClick()
        idle(activity)
        shoot(activity, name)
        activity.router.back()
        idle(activity)
    }

    private fun destinations(): List<Pair<String, Destination>> = listOf(
        "connection" to ConnectionDestination,
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

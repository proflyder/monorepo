package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.View
import android.widget.TextView
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.AppBarLayout
import com.google.android.material.bottomnavigation.BottomNavigationView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import com.google.android.material.R as MaterialR
import dev.proflyder.home.feature.control.impl.R as ControlR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class GlassToolbarScreenshotTest {
    @Test
    fun homeScrolledDark() = shootHome("dark-scrolled", SCROLL_DP)

    @Test
    fun homeAtRestDark() = shootHome("dark-rest", 0)

    @Test
    @Config(qualifiers = LIGHT)
    fun homeScrolledLight() = shootHome("light-scrolled", SCROLL_DP)

    @Test
    @Config(qualifiers = LIGHT)
    fun homeAtRestLight() = shootHome("light-rest", 0)

    @Test
    fun systemTabDark() = shootTab("dark", SYSTEM_TAB)

    @Test
    @Config(qualifiers = LIGHT)
    fun systemTabLight() = shootTab("light", SYSTEM_TAB)

    private fun shootTab(mode: String, tab: Int) {
        val activity = launch(AppTheme.EMERALD)
        val root = activity.window.decorView
        val nav = root.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        nav.selectedItemId = nav.menu.getItem(tab).itemId
        idle(activity)
        applyInsets(activity, 0)
        shoot(root, "glass-toolbar-tab$tab-$mode.png")
    }

    private fun shootHome(mode: String, scrollDp: Int) {
        val theme = AppTheme.EMERALD
        val activity = launch(theme)
        val root = activity.window.decorView
        val list = root.findViewById<RecyclerView>(ControlR.id.control_list)
        list.layoutManager = LinearLayoutManager(activity)
        list.adapter = TestSectionedRows(activity, ControlR.layout.item_control_header) { view, section ->
            view.findViewById<TextView>(ControlR.id.control_header_title).text = TITLES[section]
            view.findViewById<TextView>(ControlR.id.control_header_summary).text = SUMMARIES[section]
        }
        applyInsets(activity, 0)
        root.findViewById<View>(ControlR.id.control_error).visibility = View.GONE
        root.findViewById<View>(ControlR.id.control_content).visibility = View.VISIBLE
        list.visibility = View.VISIBLE
        layout(root)
        val scrolled = scrollDp > 0
        root.findViewById<AppBarLayout>(ControlR.id.control_appbar).setExpanded(!scrolled, false)
        list.scrollBy(0, dp(root, scrollDp))
        repeat(SETTLE_PASSES) {
            layout(root)
            idle(activity)
        }
        val shot = shoot(root, "glass-toolbar-home-$mode.png")
        val barBottom = root.findViewById<View>(R.id.appbar).height
        assertStatusBandIsSolid(shot, dp(root, STATUS_BAR_DP))
        if (scrolled) assertHeaderPinnedAtBarEdge(activity, shot, list, barBottom)
        if (!scrolled) assertInnerHeaderBelowBar(root, barBottom)
    }

    private fun assertStatusBandIsSolid(shot: Bitmap, statusBar: Int) {
        val reference = shot.getPixel(shot.width / 2, statusBar / 2)
        val deviation = (0 until statusBar).flatMap { y -> (0 until shot.width).map { x -> shot.getPixel(x, y) } }
            .maxOf { channelDistance(it, reference) }
        assertTrue("content shows through the status bar band: $deviation", deviation <= STATUS_TOLERANCE)
    }

    private fun assertHeaderPinnedAtBarEdge(activity: MainActivity, shot: Bitmap, list: RecyclerView, barBottom: Int) {
        val listTop = IntArray(2).also(list::getLocationInWindow)[1]
        assertEquals("list top sits at the glass bar edge", barBottom, listTop + list.paddingTop)
        val surface = MaterialColors.getColor(activity, MaterialR.attr.colorSurface, 0)
        val band = (barBottom until barBottom + dp(list, HEADER_PROBE_DP)).flatMap { y -> (0 until shot.width).map { x -> shot.getPixel(x, y) } }
        assertTrue("pinned header must start at the bar edge", band.all { channelDistance(it, surface) <= 1 })
    }

    private fun assertInnerHeaderBelowBar(root: View, barBottom: Int) {
        val slot = root.findViewById<View>(ControlR.id.control_alarms_slot)
        val slotTop = IntArray(2).also(slot::getLocationInWindow)[1]
        assertEquals("inner app bar content starts at the glass bar edge", barBottom, slotTop)
    }

    private fun channelDistance(first: Int, second: Int): Int = listOf(
        Color.red(first) - Color.red(second),
        Color.green(first) - Color.green(second),
        Color.blue(first) - Color.blue(second),
    ).maxOf { kotlin.math.abs(it) }

    private fun launch(theme: AppTheme): MainActivity {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        runBlocking { app.graph.appearance.selectTheme(theme) }
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        return activity
    }

    private fun applyInsets(activity: MainActivity, keyboard: Int) {
        val root = activity.window.decorView
        val builder = WindowInsetsCompat.Builder()
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, dp(root, STATUS_BAR_DP), 0, 0))
            .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, dp(root, NAV_BAR_DP)))
            .setInsets(WindowInsetsCompat.Type.ime(), Insets.of(0, 0, 0, keyboard))
            .setVisible(WindowInsetsCompat.Type.ime(), keyboard > 0)
        layout(root)
        ViewCompat.dispatchApplyWindowInsets(root, builder.build())
        repeat(SETTLE_PASSES) {
            layout(root)
            idle(activity)
        }
        layout(root)
    }

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun layout(root: View) {
        root.measure(exactly(WIDTH_DP, root), exactly(HEIGHT_DP, root))
        root.layout(0, 0, root.measuredWidth, root.measuredHeight)
    }

    private fun shoot(root: View, name: String): Bitmap {
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        System.getenv("TEST_UNDECLARED_OUTPUTS_DIR")?.let { dir ->
            File(dir).mkdirs()
            File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
        }
        return bitmap
    }

    private companion object {
        const val LIGHT = "w411dp-h891dp-notnight-xxhdpi"
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val STATUS_BAR_DP = 32
        const val NAV_BAR_DP = 24
        const val SCROLL_DP = 90
        const val SYSTEM_TAB = 4
        const val STATUS_TOLERANCE = 6
        const val HEADER_PROBE_DP = 12
        const val SETTLE_PASSES = 3
        const val PNG_QUALITY = 100
        val TITLES = listOf("Спальня", "Гостиная", "Кухня", "Прихожая")
        val SUMMARIES = listOf("Включено 2 из 8", "Включено 1 из 5", "Выключено", "Выключено")
    }
}

private fun dp(view: View, value: Int): Int = (value * view.resources.displayMetrics.density).toInt()

private fun exactly(value: Int, view: View): Int =
    View.MeasureSpec.makeMeasureSpec((value * view.resources.displayMetrics.density).toInt(), View.MeasureSpec.EXACTLY)

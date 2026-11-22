package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import android.view.ViewGroup
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.android.material.bottomnavigation.BottomNavigationView
import com.google.android.material.floatingactionbutton.ExtendedFloatingActionButton
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.designsystem.OutlinedFloatingActionButton
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowWindowManagerImpl
import dev.proflyder.home.feature.control.impl.R as ControlR
import dev.proflyder.home.feature.scenarios.impl.R as ScenariosR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class FabStyleScreenshotTest {
    @Test
    fun neon() = check(AppTheme.NEON)

    @Test
    fun current() = check(AppTheme.CURRENT)

    @Test
    fun system() = check(AppTheme.SYSTEM)

    private fun check(theme: AppTheme) {
        val activity = launch(theme)
        val root = activity.window.decorView
        val fab = root.findViewById<OutlinedFloatingActionButton>(ControlR.id.control_quick_fab)
        assertTrue("stroke in $theme", fab.strokeWidth > 0)
        assertEquals("one stroke width in every theme", dp(root, STROKE_DP), fab.strokeWidth)
        shoot("fab-${theme.key}-home.png", root)

        openDial(activity, fab)
        val dial = requireNotNull(dialRoot(activity))
        val buttons = mutableListOf<OutlinedFloatingActionButton>()
        collect(dial, buttons)
        assertEquals(DIAL_ITEMS + 1, buttons.size)
        buttons.forEach { assertSameStroke(fab, it.strokeWidth, it.strokeColor?.defaultColor) }
        buttons.forEach { assertGlowNotClipped(it, dial) }
        shoot("fab-${theme.key}-dial.png", root, dial)
        activity.onBackPressedDispatcher.onBackPressed()
        idle(activity)

        selectTab(activity, 1)
        val extended = root.findViewById<ExtendedFloatingActionButton>(ScenariosR.id.scenarios_add)
        assertSameStroke(fab, extended.strokeWidth, extended.strokeColor?.defaultColor)
        shoot("fab-${theme.key}-scenarios.png", root)
    }

    private fun assertSameStroke(fab: OutlinedFloatingActionButton, width: Int, color: Int?) {
        assertEquals("stroke width", fab.strokeWidth, width)
        assertEquals("stroke color", fab.strokeColor?.defaultColor, color)
    }

    private fun assertGlowNotClipped(button: View, dial: View) {
        val parent = button.parent as View
        when (parent) {
            dial -> Unit
            is ViewGroup -> assertNoClip(parent, dial)
            else -> error("unexpected parent")
        }
    }

    private fun assertNoClip(group: ViewGroup, dial: View) {
        assertFalse("clipChildren on ${group.javaClass.simpleName}", group.clipChildren)
        assertFalse("clipToPadding on ${group.javaClass.simpleName}", group.clipToPadding)
        val parent = group.parent as? ViewGroup ?: return
        if (group !== dial) assertNoClip(parent, dial)
    }

    private fun collect(view: View, into: MutableList<OutlinedFloatingActionButton>) {
        if (view is OutlinedFloatingActionButton) into += view
        val group = view as? ViewGroup ?: return
        (0 until group.childCount).forEach { collect(group.getChildAt(it), into) }
    }

    private fun openDial(activity: MainActivity, fab: View) {
        fab.performClick()
        idle(activity)
        val dial = requireNotNull(dialRoot(activity))
        layout(dial)
        dial.viewTreeObserver.dispatchOnPreDraw()
        shadowOf(activity.mainLooper).idleFor(java.time.Duration.ofMillis(DIAL_SETTLE_MS))
        layout(dial)
    }

    private fun dialRoot(activity: MainActivity): View? {
        val windows = (shadowOf(activity.windowManager) as ShadowWindowManagerImpl).views
        return windows.firstNotNullOfOrNull { it.findViewById<View>(ControlR.id.control_dial_root) }
    }

    private fun selectTab(activity: MainActivity, index: Int) {
        val bottom = activity.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        bottom.menu.performIdentifierAction(bottom.menu.getItem(index).itemId, 0)
        idle(activity)
        layout(activity.window.decorView)
    }

    private fun launch(theme: AppTheme): MainActivity {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        app.graph.appearance.selectTheme(theme)
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        val root = activity.window.decorView
        val insets = WindowInsetsCompat.Builder()
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, dp(root, BAR_DP), 0, 0))
            .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, dp(root, BAR_DP)))
            .build()
        layout(root)
        ViewCompat.dispatchApplyWindowInsets(root, insets)
        idle(activity)
        layout(root)
        return activity
    }

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun layout(root: View) {
        val metrics = root.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
    }

    private fun shoot(name: String, vararg layers: View) {
        val bitmap = Bitmap.createBitmap(layers[0].width, layers[0].height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        layers.forEach { it.draw(canvas) }
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val BAR_DP = 24
        const val STROKE_DP = 1
        const val DIAL_ITEMS = 2
        const val DIAL_SETTLE_MS = 800L
        const val PNG_QUALITY = 100
    }
}

private fun dp(view: View, value: Int): Int = (value * view.resources.displayMetrics.density).toInt()

private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

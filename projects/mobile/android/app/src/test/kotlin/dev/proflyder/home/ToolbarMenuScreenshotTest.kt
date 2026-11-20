package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import com.google.android.material.appbar.MaterialToolbar
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.settings.AppearanceDestination
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
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
import androidx.appcompat.R as AppCompatR
import dev.proflyder.home.core.ui.R as UiR
import dev.proflyder.home.feature.chat.impl.R as ChatR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ToolbarMenuScreenshotTest {
    @Test
    fun emerald() = shootAll(AppTheme.CURRENT)

    @Test
    fun neon() = shootAll(AppTheme.NEON)

    @Test
    fun system() = shootAll(AppTheme.SYSTEM)

    private fun shootAll(theme: AppTheme) {
        val activity = launch(theme)
        assertEquals(UiR.style.Widget_Home_PopupMenu, styleOf(activity, AppCompatR.attr.popupMenuStyle))
        assertEquals(UiR.style.Widget_Home_ListPopupWindow, styleOf(activity, AppCompatR.attr.listPopupWindowStyle))

        val toolbar = activity.findViewById<MaterialToolbar>(R.id.toolbar)
        toolbar.menu.performIdentifierAction(R.id.action_menu, 0)
        idle(activity)
        val actions = requireNotNull(activity.actionsMenu.contentView) { "the menu must be open" }
        assertSharedMenu(actions)
        shoot("menu-${theme.key}-actions-dark.png", activity, actions, toolbar.findViewById(R.id.action_menu))
        activity.actionsMenu.dismiss()
        idle(activity)

        activity.router.navigate(AppearanceDestination)
        idle(activity)
        shoot("menu-${theme.key}-appearance-dark.png", activity)

        activity.router.selectTab("chat")
        idle(activity)
        val attach = activity.findViewById<View>(ChatR.id.chat_attach)
        attach.performClick()
        idle(activity)
        val menu = requireNotNull(openPopup(activity)) { "the attach menu must be open" }
        assertSharedMenu(menu)
        assertNotNull(menu.findViewById<View>(ChatR.id.chat_attach_photo))
        assertNotNull(menu.findViewById<View>(ChatR.id.chat_attach_file))
        shoot("menu-${theme.key}-chat-attach-dark.png", activity, menu, attach)
    }

    private fun assertSharedMenu(menu: View) {
        assertEquals(UiR.id.ds_popup_menu_items, menu.id)
        val rows = menu as ViewGroup
        assertTrue(rows.childCount > 0)
        (0 until rows.childCount).map(rows::getChildAt).forEach { assertRowIcon(it) }
    }

    private fun assertRowIcon(row: View) {
        assertNotNull("row icon", row.findViewById<ImageView>(UiR.id.ds_popup_menu_icon).drawable)
    }

    private fun styleOf(activity: MainActivity, attr: Int): Int {
        val value = TypedValue()
        activity.theme.resolveAttribute(attr, value, true)
        return value.resourceId
    }

    private fun openPopup(activity: MainActivity): View? {
        val windows = (shadowOf(activity.windowManager) as ShadowWindowManagerImpl).views
        return windows.lastOrNull { it.findViewById<View>(UiR.id.ds_popup_menu_items) != null }
            ?.findViewById(UiR.id.ds_popup_menu_items)
    }

    private fun launch(theme: AppTheme): MainActivity {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        app.graph.appearance.selectTheme(theme)
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        return activity
    }

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun shoot(name: String, activity: MainActivity, menu: View? = null, anchor: View? = null) {
        val root = activity.window.decorView
        val metrics = root.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        root.draw(canvas)
        menu?.let { drawMenu(canvas, it, requireNotNull(anchor), width) }
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun drawMenu(canvas: Canvas, menu: View, anchor: View, width: Int) {
        menu.measure(unspecified(), unspecified())
        menu.layout(0, 0, menu.measuredWidth, menu.measuredHeight)
        val location = IntArray(2)
        anchor.getLocationInWindow(location)
        val left = location[0].coerceAtMost(width - menu.measuredWidth)
        val below = location[1] + anchor.height
        val top = when {
            below + menu.measuredHeight > canvas.height -> location[1] - menu.measuredHeight
            else -> below
        }
        canvas.save()
        canvas.translate(left.toFloat(), top.toFloat())
        menu.draw(canvas)
        canvas.restore()
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private fun unspecified(): Int = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED)

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val PNG_QUALITY = 100
    }
}

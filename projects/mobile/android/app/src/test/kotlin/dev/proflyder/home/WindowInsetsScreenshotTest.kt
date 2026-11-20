package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.widget.NestedScrollView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import com.google.android.material.textfield.TextInputLayout
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.connection.ConnectionDestination
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
import dev.proflyder.home.feature.alarms.impl.R as AlarmsR
import dev.proflyder.home.feature.connection.impl.R as ConnectionR
import dev.proflyder.home.feature.control.impl.R as ControlR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class WindowInsetsScreenshotTest {
    @Test
    fun saveBarSitsOnTheKeyboardInNeon() = alarmEditorWithKeyboard(AppTheme.NEON)

    @Test
    fun saveBarSitsOnTheKeyboardInCurrent() = alarmEditorWithKeyboard(AppTheme.CURRENT)

    @Test
    fun homeLastItemClearsBottomNavigationInNeon() = homeScrolledToEnd(AppTheme.NEON)

    @Test
    fun homeLastItemClearsBottomNavigationInCurrent() = homeScrolledToEnd(AppTheme.CURRENT)

    @Test
    fun connectionFieldsUseTheHomeStyleInNeon() = connectionFields(AppTheme.NEON)

    @Test
    fun connectionFieldsUseTheHomeStyleInCurrent() = connectionFields(AppTheme.CURRENT)

    private fun alarmEditorWithKeyboard(theme: AppTheme) {
        val activity = launch(theme)
        activity.router.navigate(AlarmEditorDestination())
        idle(activity)
        val root = activity.window.decorView
        val label = root.findViewById<View>(AlarmsR.id.alarm_editor_label)
        label.requestFocus()
        val keyboard = dp(root, KEYBOARD_DP)
        applyInsets(activity, keyboard)

        val bar = root.findViewById<View>(AlarmsR.id.alarm_editor_save_bar)
        assertEquals("save bar is not on the keyboard", root.height - keyboard, bar.windowBottom())
        val nav = root.findViewById<View>(R.id.home_notice_anchor)
        assertFalse("bottom navigation stays above the keyboard", nav.isShown)
        val field = root.findViewById<TextInputLayout>(AlarmsR.id.alarm_editor_label_layout)
        assertTrue("field hides behind the save bar", field.windowBottom() <= bar.windowTop())
        assertTrue("field went above the screen", field.windowTop() >= 0)
        assertHomeStyle(field)
        shoot(activity, "insets-${theme.key}-alarm-editor-keyboard.png", keyboard)
    }

    private fun homeScrolledToEnd(theme: AppTheme) {
        val activity = launch(theme)
        val root = activity.window.decorView
        val list = root.findViewById<RecyclerView>(ControlR.id.control_list)
        list.layoutManager = LinearLayoutManager(activity)
        list.adapter = RowsAdapter(ROWS)
        applyInsets(activity, 0)
        root.findViewById<View>(ControlR.id.control_error).visibility = View.GONE
        root.findViewById<View>(ControlR.id.control_content).visibility = View.VISIBLE
        list.visibility = View.VISIBLE
        list.scrollToPosition(ROWS - 1)
        layout(root)
        list.scrollBy(0, dp(root, OVERSCROLL_DP))
        layout(root)

        val last = list.findViewHolderForAdapterPosition(ROWS - 1)?.itemView
        checkNotNull(last) { "last row is not laid out" }
        assertTrue("list is not shown", last.isShown)
        val nav = root.findViewById<View>(R.id.home_notice_anchor)
        val fab = root.findViewById<View>(ControlR.id.control_quick_fab)
        assertTrue("bottom navigation is hidden", nav.isShown)
        assertTrue("last row is under the bottom navigation", last.windowBottom() <= nav.windowTop())
        assertTrue("last row is under the FAB", last.windowBottom() <= fab.windowTop())
        assertEquals(root.height - dp(root, NAV_BAR_DP), nav.windowBottom() - nav.paddingBottom)
        shoot(activity, "insets-${theme.key}-home-end.png", 0)
    }

    private fun connectionFields(theme: AppTheme) {
        val activity = launch(theme)
        activity.router.navigate(ConnectionDestination)
        idle(activity)
        applyInsets(activity, 0)
        val root = activity.window.decorView
        val field = root.findViewById<TextInputLayout>(ConnectionR.id.connection_token_layout)
        assertHomeStyle(field)
        val scroll = root.findViewById<NestedScrollView>(ConnectionR.id.connection_scroll)
        assertFalse(scroll.clipToPadding)
        shoot(activity, "insets-${theme.key}-connection.png", 0)
    }

    private fun assertHomeStyle(field: TextInputLayout) {
        val radius = field.resources.getDimension(dev.proflyder.home.core.ui.R.dimen.ui_card_radius)
        val fill = MaterialColors.getColor(field, com.google.android.material.R.attr.colorSurfaceContainerHigh)
        assertEquals(TextInputLayout.BOX_BACKGROUND_FILLED, field.boxBackgroundMode)
        assertEquals(fill, field.boxBackgroundColor)
        assertEquals(0, field.boxStrokeWidth)
        assertEquals(0, field.boxStrokeWidthFocused)
        assertEquals(radius, field.boxCornerRadiusTopStart, 0.5f)
        assertEquals(radius, field.boxCornerRadiusBottomEnd, 0.5f)
        assertTrue("no focus outline", field.editText?.foreground != null)
    }

    private fun launch(theme: AppTheme): MainActivity {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        app.graph.appearance.selectTheme(theme)
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
        val metrics = root.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
    }

    private fun shoot(activity: MainActivity, name: String, keyboard: Int) {
        val root = activity.window.decorView
        layout(root)
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        root.draw(canvas)
        val keys = MaterialColors.getColor(root, com.google.android.material.R.attr.colorSurfaceContainerHighest, 0)
        canvas.clipRect(Rect(0, root.height - keyboard, root.width, root.height))
        canvas.drawColor(keys)
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private class RowsAdapter(private val count: Int) : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val row = TextView(parent.context)
            row.layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(parent, ROW_DP))
            row.setBackgroundColor(
                MaterialColors.getColor(parent, com.google.android.material.R.attr.colorSurfaceContainer),
            )
            return object : RecyclerView.ViewHolder(row) {}
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            (holder.itemView as TextView).text = position.toString()
        }

        override fun getItemCount(): Int = count
    }

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val KEYBOARD_DP = 320
        const val STATUS_BAR_DP = 32
        const val NAV_BAR_DP = 24
        const val ROW_DP = 96
        const val ROWS = 20
        const val OVERSCROLL_DP = 2000
        const val SETTLE_PASSES = 3
        const val PNG_QUALITY = 100
    }
}

private fun dp(view: View, value: Int): Int = (value * view.resources.displayMetrics.density).toInt()

private fun View.windowTop(): Int {
    val location = IntArray(2)
    getLocationInWindow(location)
    return location[1]
}

private fun View.windowBottom(): Int = windowTop() + height

private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

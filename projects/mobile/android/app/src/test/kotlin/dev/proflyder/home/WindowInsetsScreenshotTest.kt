package dev.proflyder.home

import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.widget.NestedScrollView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import com.google.android.material.button.MaterialButton
import com.google.android.material.card.MaterialCardView
import com.google.android.material.textfield.TextInputLayout
import dev.proflyder.home.core.designsystem.CardTextInputLayout
import dev.proflyder.home.core.designsystem.R as DesignR
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.feature.chat.ChatRoute
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
import dev.proflyder.home.core.ui.R as UiR
import dev.proflyder.home.feature.alarms.impl.R as AlarmsR
import dev.proflyder.home.feature.chat.impl.R as ChatR
import dev.proflyder.home.feature.connection.impl.R as ConnectionR
import dev.proflyder.home.feature.control.impl.R as ControlR
import dev.proflyder.home.feature.push.impl.R as PushR
import dev.proflyder.home.feature.reminders.impl.R as RemindersR
import dev.proflyder.home.feature.scenarios.impl.R as ScenariosR
import dev.proflyder.home.feature.timers.impl.R as TimersR
import kotlinx.coroutines.runBlocking

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class WindowInsetsScreenshotTest {
    @Test
    fun saveBarSitsOnTheKeyboardInNeon() = alarmEditorWithKeyboard(AppTheme.NEON)

    @Test
    fun saveBarSitsOnTheKeyboardInCurrent() = alarmEditorWithKeyboard(AppTheme.CURRENT)

    @Test
    @Config(qualifiers = LIGHT)
    fun saveBarSitsOnTheKeyboardInNeonLight() = alarmEditorWithKeyboard(AppTheme.NEON)

    @Test
    @Config(qualifiers = LIGHT)
    fun saveBarSitsOnTheKeyboardInCurrentLight() = alarmEditorWithKeyboard(AppTheme.CURRENT)

    @Test
    @Config(qualifiers = RU_DARK)
    fun pushNameFieldAndTitleInNeon() = pushSettings(AppTheme.NEON)

    @Test
    @Config(qualifiers = RU_DARK)
    fun pushNameFieldAndTitleInCurrent() = pushSettings(AppTheme.CURRENT)

    @Test
    @Config(qualifiers = RU_LIGHT)
    fun pushNameFieldAndTitleInNeonLight() = pushSettings(AppTheme.NEON)

    @Test
    @Config(qualifiers = RU_LIGHT)
    fun pushNameFieldAndTitleInCurrentLight() = pushSettings(AppTheme.CURRENT)

    @Test
    @Config(qualifiers = LIGHT)
    fun connectionFieldsUseTheHomeStyleInNeonLight() = connectionFields(AppTheme.NEON)

    @Test
    @Config(qualifiers = LIGHT)
    fun connectionFieldsUseTheHomeStyleInCurrentLight() = connectionFields(AppTheme.CURRENT)

    @Test
    fun everyFieldInTheAppUsesTheHomeStyle() {
        val activity = launch(AppTheme.NEON)
        val fields = FIELD_LAYOUTS.flatMap { inflateFields(activity, it) }
        assertTrue("expected every app field, found ${fields.size}", fields.size >= FIELD_COUNT)
        fields.forEach(::assertHomeStyle)
    }

    @Test
    fun homeLastItemClearsBottomNavigationInNeon() = homeScrolledToEnd(AppTheme.NEON)

    @Test
    fun homeLastItemClearsBottomNavigationInCurrent() = homeScrolledToEnd(AppTheme.CURRENT)

    @Test
    fun connectionFieldsUseTheHomeStyleInNeon() = connectionFields(AppTheme.NEON)

    @Test
    fun connectionFieldsUseTheHomeStyleInCurrent() = connectionFields(AppTheme.CURRENT)

    @Test
    fun chatPanelSitsOnTheKeyboardInNeon() = chatWithKeyboard(AppTheme.NEON)

    @Test
    fun chatPanelSitsOnTheKeyboardInCurrent() = chatWithKeyboard(AppTheme.CURRENT)

    private fun chatWithKeyboard(theme: AppTheme) {
        val activity = launch(theme)
        activity.router.selectTab(ChatRoute.FEATURE_ID)
        idle(activity)
        val root = activity.window.decorView
        root.findViewById<View>(ChatR.id.chat_input).requestFocus()
        val keyboard = dp(root, KEYBOARD_DP)
        applyInsets(activity, keyboard)

        val panel = root.findViewById<View>(ChatR.id.chat_panel)
        assertEquals("chat panel is not on the keyboard", root.height - keyboard, panel.windowBottom())
        val nav = root.findViewById<View>(R.id.home_notice_anchor)
        assertFalse("bottom navigation stays above the keyboard", nav.isShown)
        assertComposerStyle(root.findViewById(ChatR.id.chat_input_layout))
        shoot(activity, "insets-${theme.key}-chat-keyboard.png", keyboard)
    }

    private fun alarmEditorWithKeyboard(theme: AppTheme) {
        val activity = launch(theme)
        activity.router.selectTab("alarms")
        idle(activity)
        activity.findViewById<View>(AlarmsR.id.alarms_add).performClick()
        idle(activity)
        val root = activity.window.decorView
        val label = root.findViewById<TextView>(AlarmsR.id.alarm_editor_label)
        val field = root.findViewById<TextInputLayout>(AlarmsR.id.alarm_editor_label_layout)
        label.text = ""
        applyInsets(activity, 0)
        assertOutline(field, focused = false)
        shoot(activity, "insets-${theme.key}-${mode(root)}-alarm-editor-empty.png", 0)
        label.requestFocus()
        applyInsets(activity, 0)
        assertOutline(field, focused = true)
        shoot(activity, "insets-${theme.key}-${mode(root)}-alarm-editor-empty-focused.png", 0)
        label.text = activity.getString(AlarmsR.string.alarms_label_default)
        val keyboard = dp(root, KEYBOARD_DP)
        applyInsets(activity, keyboard)

        val bar = root.findViewById<View>(AlarmsR.id.alarm_editor_save_bar)
        assertEquals("save bar is not on the keyboard", root.height - keyboard, bar.windowBottom())
        val save = root.findViewById<View>(AlarmsR.id.alarm_editor_save)
        assertEquals("save button height", root.resources.getDimensionPixelSize(UiR.dimen.ds_control_l), save.height)
        val nav = root.findViewById<View>(R.id.home_notice_anchor)
        assertFalse("bottom navigation stays above the keyboard", nav.isShown)
        assertTrue("field hides behind the save bar", field.windowBottom() <= bar.windowTop())
        assertTrue("field went above the screen", field.windowTop() >= 0)
        assertHomeStyle(field)
        assertOutline(field, focused = true)
        shoot(activity, "insets-${theme.key}-${mode(root)}-alarm-editor-keyboard.png", keyboard)
        label.clearFocus()
        applyInsets(activity, 0)
        assertOutline(field, focused = false)
        shoot(activity, "insets-${theme.key}-${mode(root)}-alarm-editor-filled.png", 0)
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
        activity.router.openRoute(CONNECTION_ROUTE)
        idle(activity)
        applyInsets(activity, 0)
        val root = activity.window.decorView
        val field = root.findViewById<TextInputLayout>(ConnectionR.id.connection_token_layout)
        assertHomeStyle(field)
        val scroll = root.findViewById<NestedScrollView>(ConnectionR.id.connection_scroll)
        assertFalse(scroll.clipToPadding)
        assertCardsAndButtons(root.findViewById(ConnectionR.id.connection_scroll))
        assertFalse("clear token shown without a token", root.findViewById<View>(ConnectionR.id.connection_clear_token).isShown)
        shoot(activity, "insets-${theme.key}-${mode(root)}-connection.png", 0)
    }

    private fun pushSettings(theme: AppTheme) {
        val activity = launch(theme)
        activity.router.openRoute(PUSH_SETTINGS_ROUTE)
        idle(activity)
        val root = activity.window.decorView
        val name = root.findViewById<TextView>(PushR.id.push_device_name)
        name.text = SAMPLE_DEVICE_NAME
        applyInsets(activity, 0)
        assertHomeStyle(name.parentField())
        assertTitleFits(root.findViewById(R.id.toolbar_title))
        assertCardsAndButtons(root.findViewById<View>(PushR.id.push_intro).parent as View)
        shoot(activity, "insets-${theme.key}-${mode(root)}-push-settings.png", 0)
    }

    private fun assertTitleFits(title: TextView) {
        val layout = checkNotNull(title.layout) { "toolbar title is not laid out" }
        assertTrue("toolbar title is empty", title.text.isNotBlank())
        (0 until layout.lineCount).forEach { line ->
            assertEquals("toolbar title is ellipsized", 0, layout.getEllipsisCount(line))
            val room = title.width - title.totalPaddingLeft - title.totalPaddingRight + 1
            assertTrue("toolbar title is clipped", layout.getLineWidth(line) <= room)
        }
        (1 until layout.lineCount).forEach { line ->
            assertTrue("toolbar title breaks inside a word", title.text[layout.getLineStart(line) - 1].isWhitespace())
        }
        assertTrue("toolbar title is taller than the toolbar", layout.height <= title.height)
    }

    private fun inflateFields(activity: MainActivity, layoutRes: Int): List<TextInputLayout> {
        val view = LayoutInflater.from(activity).inflate(layoutRes, FrameLayout(activity), false)
        val width = (WIDTH_DP * activity.resources.displayMetrics.density).toInt()
        return view.fields().onEach { it.measureAlone(width) }
    }

    private fun assertComposerStyle(field: TextInputLayout) {
        assertEquals(TextInputLayout.BOX_BACKGROUND_OUTLINE, field.boxBackgroundMode)
        assertEquals("no underline at rest", 0, field.boxStrokeWidth)
        assertTrue("thin accent outline in focus", field.boxStrokeWidthFocused > 0)
        assertEquals(field.boxCornerRadiusTopStart, field.boxCornerRadiusBottomEnd)
        assertFalse("the composer uses a placeholder, not a floating label", field.isHintEnabled)
        assertFalse(checkNotNull(field.editText).hint.isNullOrBlank())
    }

    private fun assertHomeStyle(field: TextInputLayout) {
        val name = field.resources.getResourceEntryName(checkNotNull(field.editText).id)
        val radius = field.resources.getDimension(UiR.dimen.ds_radius_l)
        assertTrue("$name is not the card field", field is CardTextInputLayout)
        assertEquals(name, TextInputLayout.BOX_BACKGROUND_FILLED, field.boxBackgroundMode)
        assertEquals("$name has an underline", 0, field.boxStrokeWidth)
        assertEquals("$name has a focused underline", 0, field.boxStrokeWidthFocused)
        val corners = listOf(
            field.boxCornerRadiusTopStart,
            field.boxCornerRadiusTopEnd,
            field.boxCornerRadiusBottomStart,
            field.boxCornerRadiusBottomEnd,
        )
        corners.forEach { assertEquals("$name corner", radius, it, CORNER_TOLERANCE) }
        assertTrue("$name has no label", field.isHintEnabled && !field.hint.isNullOrBlank())
        assertLabelInside(name, field)
        assertTextInside(name, checkNotNull(field.editText))
    }

    private fun assertOutline(field: TextInputLayout, focused: Boolean) {
        val card = field as CardTextInputLayout
        val res = field.resources
        val width = when {
            focused -> res.getDimensionPixelSize(UiR.dimen.ds_stroke_thick)
            else -> res.getDimensionPixelSize(UiR.dimen.ds_stroke_thin)
        }
        val color = when {
            focused -> MaterialColors.getColor(field, R.attr.colorStateOn)
            else -> MaterialColors.getColor(field, com.google.android.material.R.attr.colorOutlineVariant)
        }
        assertEquals("outline width", width, card.outlineWidth)
        assertEquals("outline color", color, card.outlineColor)
    }

    private fun assertLabelInside(name: String, field: TextInputLayout) {
        val edit = checkNotNull(field.editText)
        val labelTop = edit.paddingTop - field.resources.displayMetrics.density * LABEL_ROOM_DP
        assertTrue("$name label crosses the outline", labelTop >= 0)
    }

    private fun assertCardsAndButtons(content: View) {
        val cards = content.all<MaterialCardView>()
        assertTrue("no cards", cards.isNotEmpty())
        val color = MaterialColors.getColor(content, com.google.android.material.R.attr.colorSurfaceContainer)
        cards.forEach { assertCard(it, color) }
        val height = content.resources.getDimensionPixelSize(UiR.dimen.ds_control_m)
        cards.forEach { card -> card.all<MaterialButton>().filter { it.isShown && !it.text.isNullOrEmpty() }.forEach { assertCardButton(card, it, height) } }
    }

    private fun assertCard(card: MaterialCardView, color: Int) {
        assertEquals("card has a stroke", 0, card.strokeWidth)
        assertEquals("card background", color, card.cardBackgroundColor.defaultColor)
    }

    private fun assertCardButton(card: MaterialCardView, button: MaterialButton, height: Int) {
        val content = card.getChildAt(0)
        val name = button.resources.getResourceEntryName(button.id)
        val room = content.width - content.paddingLeft - content.paddingRight
        val minWidth = button.resources.getDimensionPixelSize(UiR.dimen.ds_button_min_width)
        assertTrue("$name is full width", button.width < room)
        assertTrue("$name narrower than the minimum", button.width >= minWidth)
        assertEquals("$name height", height, button.height)
        assertEquals("$name corner", button.resources.getDimensionPixelSize(UiR.dimen.ds_radius_l), button.cornerRadius)
        assertTrue("$name starts before the content edge", button.windowLeft() - content.windowLeft() >= content.paddingLeft)
    }

    private fun assertTextInside(name: String, edit: TextView) {
        val layout = checkNotNull(edit.layout) { "$name is not laid out" }
        val top = edit.totalPaddingTop
        val bottom = top + layout.getLineBottom(0)
        assertTrue("$name text starts above the field", top >= 0)
        assertTrue("$name text goes below the field", bottom <= edit.height)
        assertTrue("$name text is pushed down", top <= edit.height - bottom + LABEL_ROOM_DP * edit.resources.displayMetrics.density)
    }

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
        const val LIGHT = "w411dp-h891dp-notnight-xxhdpi"
        const val RU_DARK = "ru-w411dp-h891dp-night-xxhdpi"
        const val RU_LIGHT = "ru-w411dp-h891dp-notnight-xxhdpi"
        const val FIELD_COUNT = 12
        const val LABEL_ROOM_DP = 20
        const val CORNER_TOLERANCE = 0.5f
        val FIELD_LAYOUTS = listOf(
            AlarmsR.layout.fragment_alarm_editor,
            ConnectionR.layout.fragment_connection,
            ControlR.layout.dialog_speak,
            PushR.layout.fragment_push_settings,
            RemindersR.layout.fragment_reminder_editor,
            ScenariosR.layout.fragment_scenario_editor,
            ScenariosR.layout.dialog_scenario_speak,
            DesignR.layout.ds_light_params_editor,
            ScenariosR.layout.dialog_scenario_entity_condition,
            ScenariosR.layout.dialog_scenario_delay,
            TimersR.layout.fragment_timers,
        )
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

private fun View.windowLeft(): Int {
    val location = IntArray(2)
    getLocationInWindow(location)
    return location[0]
}

private inline fun <reified T : View> View.all(): List<T> = tree().filterIsInstance<T>()

private fun View.tree(): List<View> = when (this) {
    is ViewGroup -> listOf(this) + (0 until childCount).flatMap { getChildAt(it).tree() }
    else -> listOf(this)
}

private fun View.windowBottom(): Int = windowTop() + height

private fun unspecified(): Int = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED)

private fun mode(view: View): String = when (view.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) {
    Configuration.UI_MODE_NIGHT_YES -> "dark"
    else -> "light"
}

private fun View.parentField(): TextInputLayout = generateSequence(parent) { it.parent }
    .filterIsInstance<TextInputLayout>()
    .first()

private fun View.measureAlone(width: Int) {
    measure(exactly(width), unspecified())
    layout(0, 0, measuredWidth, measuredHeight)
}

private fun View.fields(): List<TextInputLayout> = when (this) {
    is TextInputLayout -> listOf(this)
    is ViewGroup -> (0 until childCount).flatMap { getChildAt(it).fields() }
    else -> emptyList()
}

private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

private const val CONNECTION_ROUTE = "connection/settings"
private const val PUSH_SETTINGS_ROUTE = "push/settings"

private const val SAMPLE_DEVICE_NAME = "Stas"

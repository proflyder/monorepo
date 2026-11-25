package dev.proflyder.home.feature.settings.presentation

import android.content.res.ColorStateList
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.PointF
import android.graphics.drawable.RippleDrawable
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import androidx.core.content.ContextCompat
import androidx.recyclerview.widget.LinearLayoutManager
import com.google.android.material.shape.MaterialShapeDrawable
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.navigation.routeWhenAllowed
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentNotificationHistoryBinding
import java.io.File
import java.time.LocalDate
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import dev.proflyder.home.core.designsystem.R as DesignR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NotificationHistoryRowTest {
    @Test
    fun rowsWithATargetShowAChevronAndHandleClicks() {
        val callback = RecordingCallback()
        val binding = list(dark(), callback)

        val open = row(binding, OPEN_ROW)
        val closed = row(binding, CLOSED_ROW)

        assertEquals(View.VISIBLE, open.findViewById<View>(R.id.history_item_chevron).visibility)
        assertEquals(View.GONE, closed.findViewById<View>(R.id.history_item_chevron).visibility)
        assertTrue(open.isClickable)
        assertFalse(closed.isClickable)
        assertTrue(open.foreground is RippleDrawable)
        assertNull(closed.foreground)

        assertTrue(open.performClick())
        assertEquals("open", callback.clicked)
        assertFalse(closed.performClick())
        assertEquals(1, callback.clicks)
    }

    @Test
    fun aPressedRowDrawsTheRippleInTheDarkTheme() = shootPressed("dark", dark())

    @Test
    @Config(qualifiers = "w411dp-h891dp-notnight-xxhdpi")
    fun aPressedRowDrawsTheRippleInTheLightTheme() =
        shootPressed("light", ContextThemeWrapper(RuntimeEnvironment.getApplication(), LIGHT_THEME))

    private fun shootPressed(mode: String, themed: ContextThemeWrapper) =
        mapOf("first" to OPEN_ROW, "last" to LAST_ROW).forEach { (name, position) -> shootPressed("$name-$mode", themed, position) }

    private fun shootPressed(name: String, themed: ContextThemeWrapper, position: Int) {
        val binding = list(themed, RecordingCallback())
        val pressed = row(binding, position)
        val bitmap = Bitmap.createBitmap(binding.root.width, binding.root.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        binding.root.draw(canvas)
        drawPressedLayer(canvas, binding.root, pressed)
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, "notification-history-pressed-$name.png").outputStream()
            .use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun drawPressedLayer(canvas: Canvas, root: View, row: View) {
        val pressed = ContextCompat.getColorStateList(row.context, DesignR.color.ds_row_item_ripple)
            ?.getColorForState(intArrayOf(android.R.attr.state_pressed), android.graphics.Color.TRANSPARENT)
            ?: return
        val mask = (row.foreground as RippleDrawable).findDrawableByLayerId(android.R.id.mask) as MaterialShapeDrawable
        val offset = offsetIn(root, row)
        MaterialShapeDrawable(mask.shapeAppearanceModel).apply {
            fillColor = ColorStateList.valueOf(pressed)
            setBounds(offset.x.toInt(), offset.y.toInt(), offset.x.toInt() + row.width, offset.y.toInt() + row.height)
            draw(canvas)
        }
    }

    private fun windowBackground(themed: ContextThemeWrapper): android.graphics.drawable.Drawable? {
        val value = android.util.TypedValue()
        themed.theme.resolveAttribute(android.R.attr.windowBackground, value, true)
        return ContextCompat.getDrawable(themed, value.resourceId)
    }

    private fun offsetIn(root: View, view: View): PointF {
        var x = 0f
        var y = 0f
        var current: View? = view
        while (current != null && current !== root) {
            x += current.left
            y += current.top
            current = current.parent as? View
        }
        return PointF(x, y)
    }

    private fun list(themed: ContextThemeWrapper, callback: RecordingCallback): FragmentNotificationHistoryBinding {
        val binding = FragmentNotificationHistoryBinding.inflate(LayoutInflater.from(themed))
        binding.historyList.layoutManager = LinearLayoutManager(themed)
        binding.historyList.adapter = NotificationHistoryAdapter(callback).apply { submitList(rows()) }
        binding.historyList.addHistoryDecorations()
        binding.historyEmpty.visibility = View.GONE
        binding.historyProgress.visibility = View.GONE
        binding.root.background = windowBackground(themed)
        val metrics = themed.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        binding.root.measure(exactly(width), exactly(height))
        binding.root.layout(0, 0, width, height)
        return binding
    }

    private fun rows(): List<HistoryRow> {
        val now = System.currentTimeMillis() / MILLIS_PER_SECOND
        val entries = listOf(
            entry("open", now, "home://chat/message/m_1"),
            entry("lost", now - MINUTE_SECONDS, null),
            entry("last", now - 2 * MINUTE_SECONDS, "home://alarms"),
        )
        val targets = testTargets()
        return HistoryRows.of(entries, LocalDate.now(), ZoneId.systemDefault()) { item ->
            targets.of(item.deepLink)?.routeWhenAllowed(AppAccessState.Unknown)
        }
    }

    private fun entry(id: String, at: Long, deepLink: String?) = NotificationHistory(
        id = id,
        type = NotificationHistoryType.NOTIFY,
        title = "Дверь $id",
        body = "Кто-то у двери",
        deepLink = deepLink,
        createdUnix = at,
    )

    private fun row(binding: FragmentNotificationHistoryBinding, position: Int): View =
        checkNotNull(binding.historyList.findViewHolderForAdapterPosition(position)).itemView

    private fun dark(): ContextThemeWrapper =
        ContextThemeWrapper(RuntimeEnvironment.getApplication(), DARK_THEME)

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private class RecordingCallback : BaseAdapterCallback<HistoryRow>(), NotificationHistoryAdapterCallback {
        var clicked: String? = null
        var clicks: Int = 0

        override fun onItemClick(item: HistoryRow) {
            clicked = item.key.substringAfter(':')
            clicks += 1
        }
    }

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val PNG_QUALITY = 100
        const val MILLIS_PER_SECOND = 1_000L
        const val MINUTE_SECONDS = 60L
        const val OPEN_ROW = 1
        const val CLOSED_ROW = 2
        const val LAST_ROW = 3
        const val DARK_THEME = com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar
        const val LIGHT_THEME = com.google.android.material.R.style.Theme_Material3_Light_NoActionBar
    }
}

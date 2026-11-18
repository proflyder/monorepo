package dev.proflyder.home.feature.alarms.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding
import java.io.File
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The dense card is one time/text line at rest: the snooze/stop row appears
 * only while the server reports a live occurrence. At 360dp and 411dp, with
 * the 1.15 large font too, the idle card stays under 112dp, and the delete
 * icon keeps its circular ripple inside the rounded card outline.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-rRU-w360dp-h800dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlarmItemLayoutTest {
    @Test
    fun idleCardIsCompactAt360dp() = assertIdleCard(1f, "alarm-card-360dp-ru.png")

    @Test
    fun idleCardIsCompactAt360dpWithLargeFont() = assertIdleCard(1.15f, null)

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun idleCardIsCompactAt411dp() = assertIdleCard(1f, "alarm-card-411dp-ru.png")

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun idleCardIsCompactAt411dpWithLargeFont() = assertIdleCard(1.15f, null)

    @Test
    fun activeCardShowsOneActionsRowAt360dp() =
        assertActiveCard(1f, "alarm-card-active-360dp-ru.png")

    @Test
    fun activeCardShowsOneActionsRowAt360dpWithLargeFont() = assertActiveCard(1.15f, null)

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun activeCardShowsOneActionsRowAt411dp() =
        assertActiveCard(1f, "alarm-card-active-411dp-ru.png")

    @Test
    fun selectionCardReplacesSwitchWithIndicatorAt360dp() =
        assertSelectionCard(1f, "alarm-card-selection-360dp-ru.png")

    @Test
    fun selectionCardReplacesSwitchWithIndicatorAt360dpWithLargeFont() =
        assertSelectionCard(1.15f, null)

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun selectionCardReplacesSwitchWithIndicatorAt411dp() =
        assertSelectionCard(1f, "alarm-card-selection-411dp-ru.png")

    private fun assertIdleCard(fontScale: Float, pngName: String?) {
        val binding = inflateAndBind(fontScale, schedule(active = false))
        val cardWidth = cardWidth(binding.root)

        assertEquals("font scale not applied to the inflated card", fontScale, appliedFontScale(binding), 0.001f)
        assertEquals("idle card still shows the actions row", View.GONE, binding.alarmActions.visibility)
        assertTrue(
            "idle card is ${heightDp(binding.root)}dp tall, expected <= 112dp",
            heightDp(binding.root) <= MAX_IDLE_HEIGHT_DP,
        )
        assertEquals("label must stay on one line", 1, binding.alarmLabel.maxLines)
        assertEquals("summary must stay on one line", 1, binding.alarmSummary.maxLines)
        assertEquals("label wrapped: ${binding.alarmLabel.lineCount}", 1, binding.alarmLabel.lineCount)
        assertEquals("summary wrapped: ${binding.alarmSummary.lineCount}", 1, binding.alarmSummary.lineCount)
        assertDeleteRippleInsideCard(binding, cardWidth)
        when (pngName) {
            null -> Unit
            else -> savePng(binding.root, pngName)
        }
    }

    private fun assertActiveCard(fontScale: Float, pngName: String?) {
        val nowUnix = Instant.now().epochSecond
        val binding = inflateAndBind(fontScale, schedule(active = true, nowUnix = nowUnix))
        val cardWidth = cardWidth(binding.root)

        assertEquals("font scale not applied to the inflated card", fontScale, appliedFontScale(binding), 0.001f)
        assertEquals("active card hides snooze/stop", View.VISIBLE, binding.alarmActions.visibility)
        assertEquals("snooze and stop split into rows", binding.alarmSnooze.top, binding.alarmStop.top)
        assertTrue(
            "stop overlaps snooze: ${binding.alarmSnooze.right} > ${binding.alarmStop.left}",
            binding.alarmSnooze.right <= binding.alarmStop.left,
        )
        assertEquals("actions row is not one button high", binding.alarmStop.height, binding.alarmActions.height)
        assertTrue(
            "snooze does not fit into one row: ${binding.alarmSnooze.right} > $cardWidth",
            binding.alarmSnooze.right <= cardWidth,
        )
        assertDeleteRippleInsideCard(binding, cardWidth)
        when (pngName) {
            null -> Unit
            else -> savePng(binding.root, pngName)
        }
    }

    /**
     * In selection mode the card drops its switch and delete: a dimmed switch
     * would read as the alarm's real state. The right edge belongs to the
     * circular check, which the card padding keeps clear of the rounded
     * corner, and the labels still fit on one line.
     */
    private fun assertSelectionCard(fontScale: Float, pngName: String?) {
        val binding = inflateAndBind(
            fontScale = fontScale,
            schedule = schedule(active = false),
            actionsAllowed = false,
            selected = true,
        )
        val cardWidth = cardWidth(binding.root)

        assertEquals("font scale not applied to the inflated card", fontScale, appliedFontScale(binding), 0.001f)
        assertEquals("switch must be gone while selecting", View.GONE, binding.alarmEnabled.visibility)
        assertEquals("delete must be gone while selecting", View.GONE, binding.alarmDelete.visibility)
        assertEquals(
            "selection indicator must be visible",
            View.VISIBLE,
            binding.alarmSelectionIndicator.visibility,
        )
        assertEquals("label wrapped: ${binding.alarmLabel.lineCount}", 1, binding.alarmLabel.lineCount)
        assertEquals("summary wrapped: ${binding.alarmSummary.lineCount}", 1, binding.alarmSummary.lineCount)

        // The card is checkable for accessibility, but its built-in corner
        // check is off: the only selection mark is the circular indicator.
        binding.root.isChecked = true
        assertEquals("card must not draw its own corner check", null, binding.root.checkedIcon)

        val indicator = binding.alarmSelectionIndicator
        val endGap = cardWidth - indicator.right
        assertTrue("indicator left ${indicator.left} outside card", indicator.left >= 0)
        assertTrue("indicator bottom ${indicator.bottom} > card ${binding.root.height}", indicator.bottom <= binding.root.height)
        assertTrue(
            "indicator touches the rounded corner: end gap $endGap < radius ${binding.root.radius}",
            endGap >= binding.root.radius,
        )
        when (pngName) {
            null -> Unit
            else -> savePng(binding.root, pngName)
        }
    }

    /** The ripple is a circle bounded by the square button, so the gap to the
     * card edge must cover its radius: then no part of it reaches the rounded
     * corner, which the card outline would clip. */
    private fun assertDeleteRippleInsideCard(binding: ItemAlarmBinding, cardWidth: Int) {
        val delete = binding.alarmDelete
        val radius = delete.width / 2
        val endGap = cardWidth - delete.right
        assertTrue("delete bottom ${delete.bottom} > card ${binding.root.height}", delete.bottom <= binding.root.height)
        assertTrue("delete left ${delete.left} outside card", delete.left >= 0)
        assertTrue("delete ripple clipped: end gap $endGap < radius $radius", endGap >= radius)
    }

    private fun inflateAndBind(
        fontScale: Float,
        schedule: Schedule,
        actionsAllowed: Boolean = true,
        selected: Boolean = false,
    ): ItemAlarmBinding {
        RuntimeEnvironment.setFontScale(fontScale)
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
        )
        val binding = ItemAlarmBinding.inflate(LayoutInflater.from(themed))
        bindAlarmCard(
            binding = binding,
            schedule = schedule,
            context = themed,
            nowUnix = schedule.activeOccurrenceUnix.coerceAtLeast(Instant.now().epochSecond),
            actionsAllowed = actionsAllowed,
            selected = selected,
        )

        val width = cardWidth(binding.root)
        binding.root.measure(exactly(width), unspecified())
        binding.root.layout(0, 0, width, binding.root.measuredHeight)
        return binding
    }

    private fun schedule(active: Boolean, nowUnix: Long = 0): Schedule = Schedule(
        id = "alarm-1",
        label = "Работа",
        kind = ScheduleKind.ALARM,
        timezone = "Asia/Almaty",
        at = WallTime(10, 0),
        onceDate = "",
        repeat = RepeatKind.WEEKDAYS,
        weekdays = listOf(1, 2, 3, 4, 5),
        nextDueUnix = if (active) 0 else tomorrowAtTen(),
        enabled = true,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
        activeOccurrenceUnix = if (active) nowUnix - 60 else 0,
        activeUntilUnix = if (active) nowUnix + 3600 else 0,
    )

    /** Keeps the review PNG close to the real row: "Будни · далее завтра 10:00". */
    private fun tomorrowAtTen(): Long {
        val zone = ZoneId.of("Asia/Almaty")
        val tomorrow = LocalDate.now(zone).plusDays(1)
        return LocalDateTime.of(tomorrow, LocalTime.of(10, 0)).atZone(zone).toEpochSecond()
    }

    private fun appliedFontScale(binding: ItemAlarmBinding): Float =
        binding.root.resources.configuration.fontScale

    private fun heightDp(view: View): Float = view.measuredHeight / view.resources.displayMetrics.density

    private fun cardWidth(root: View): Int {
        val margin = root.resources.getDimensionPixelSize(R.dimen.alarms_screen_margin)
        return root.resources.displayMetrics.widthPixels - margin * 2
    }

    private fun savePng(root: View, name: String) {
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private fun unspecified(): Int = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED)

    private companion object {
        const val MAX_IDLE_HEIGHT_DP = 112f
    }
}

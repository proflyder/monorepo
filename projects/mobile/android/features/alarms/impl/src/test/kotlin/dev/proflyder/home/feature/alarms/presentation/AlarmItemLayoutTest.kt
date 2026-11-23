package dev.proflyder.home.feature.alarms.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.RectF
import android.os.Looper
import android.util.TypedValue
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.button.MaterialButton
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding
import java.io.File
import java.time.Duration
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
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

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
    fun pressedDeleteDrawsRoundStateLayerAt360dp() =
        assertPressedDelete(1f, "alarm-card-delete-pressed-360dp-ru.png")

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun pressedDeleteDrawsRoundStateLayerAt411dp() =
        assertPressedDelete(1f, "alarm-card-delete-pressed-411dp-ru.png")

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
        assertDeleteButton(binding, cardWidth)
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
        assertDeleteButton(binding, cardWidth)
        when (pngName) {
            null -> Unit
            else -> savePng(binding.root, pngName)
        }
    }

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

        binding.root.isChecked = true
        assertEquals("card must not draw its own corner check", null, binding.root.checkedIcon)

        val indicator = binding.alarmSelectionIndicator
        val check = binding.alarmSelectionCheck
        val (indicatorLeft, indicatorTop) = offsetInRoot(binding.root, indicator)
        assertTrue("indicator left $indicatorLeft outside card", indicatorLeft >= 0)
        assertTrue("indicator top $indicatorTop is above the card", indicatorTop >= 0)
        assertTrue(
            "indicator bottom ${indicatorTop + indicator.height} > card ${binding.root.height}",
            indicatorTop + indicator.height <= binding.root.height,
        )
        assertEquals(
            "selection check must match the delete glyph size",
            dp(check, R.dimen.alarms_card_icon_size),
            check.width,
        )
        assertTrue(
            "selection check touches the card edge: end gap ${cardWidth - indicatorLeft - indicator.width}",
            cardWidth - indicatorLeft - indicator.width >= rippleGap(indicator),
        )
        when (pngName) {
            null -> Unit
            else -> savePng(binding.root, pngName)
        }
    }

    private fun assertDeleteButton(binding: ItemAlarmBinding, cardWidth: Int) {
        val delete = binding.alarmDelete
        val switch = binding.alarmEnabled
        val buttonSize = dp(delete, R.dimen.alarms_card_icon_button_size)
        val iconSize = dp(delete, R.dimen.alarms_card_icon_size)
        val gap = dp(delete, R.dimen.alarms_card_trailing_gap)

        assertEquals("delete touch target width is not 48dp", buttonSize, delete.width)
        assertEquals("delete touch target height is not 48dp", buttonSize, delete.height)
        assertEquals("delete glyph must be 24dp", iconSize, delete.icon?.bounds?.width())
        assertEquals("delete glyph must be 24dp", iconSize, delete.icon?.bounds?.height())
        val (glyphLeft, glyphTop) = glyphOffsetInButton(delete)
        assertEquals("delete glyph is not centred horizontally", (buttonSize - iconSize) / 2, glyphLeft)
        assertEquals("delete glyph is not centred vertically", (buttonSize - iconSize) / 2, glyphTop)
        assertEquals(
            "switch and delete are not on one vertical centre",
            switch.top + switch.height / 2,
            delete.top + delete.height / 2,
        )
        assertEquals("switch and delete gap is not 4dp", gap, delete.left - switch.right)

        val margin = rippleGap(delete)
        val (deleteLeft, deleteTop) = offsetInRoot(binding.root, delete)
        val rippleLeft = deleteLeft - margin
        val rippleTop = deleteTop - margin
        val rippleRight = deleteLeft + delete.width + margin
        val rippleBottom = deleteTop + delete.height + margin
        assertTrue("delete ripple starts at $rippleLeft, outside the card", rippleLeft >= 0)
        assertTrue("delete ripple top $rippleTop is above the card", rippleTop >= 0)
        assertTrue("delete ripple ends at $rippleRight, past the card width $cardWidth", rippleRight <= cardWidth)
        assertTrue(
            "delete ripple bottom $rippleBottom is below the card height ${binding.root.height}",
            rippleBottom <= binding.root.height,
        )
    }

    private fun glyphOffsetInButton(button: MaterialButton): Pair<Int, Int> {
        val bounds = checkNotNull(button.icon).bounds
        val verticalSpace = button.height - button.compoundPaddingTop - button.compoundPaddingBottom
        val top = button.compoundPaddingTop + (verticalSpace - bounds.height()) / 2 + bounds.top
        return button.paddingLeft + bounds.left to top
    }

    private fun offsetInRoot(root: View, view: View): Pair<Int, Int> {
        var left = 0
        var top = 0
        var current: View? = view
        while (current != null && current !== root) {
            left += current.left
            top += current.top
            current = current.parent as? View
        }
        return left to top
    }

    private fun assertPressedDelete(fontScale: Float, pngName: String) {
        val binding = inflateAndBind(fontScale, schedule(active = false))
        assertDeleteButton(binding, cardWidth(binding.root))

        val delete = binding.alarmDelete
        assertRippleColor(delete)
        delete.isPressed = true
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(RIPPLE_SETTLE_MS))
        assertTrue(
            "delete is not in the pressed state",
            delete.drawableState.contains(android.R.attr.state_pressed),
        )
        savePng(binding.root, pngName)
    }

    private fun assertRippleColor(delete: MaterialButton) {
        val ripple = delete.rippleColor
        assertTrue("delete has no ripple color", ripple != null)
        when (ripple) {
            null -> Unit
            else -> {
                val error = MaterialColors.getColor(delete, com.google.android.material.R.attr.colorError)
                assertEquals(
                    "ripple hue must match the error color",
                    error and RGB_MASK,
                    ripple.defaultColor and RGB_MASK,
                )
                val alpha = Color.alpha(ripple.defaultColor)
                assertTrue("ripple alpha $alpha is not a state layer", alpha in 1 until 0xFF)
            }
        }
        val shape = delete.shapeAppearanceModel
        val bounds = RectF(0f, 0f, delete.width.toFloat(), delete.height.toFloat())
        listOf(shape.topLeftCornerSize, shape.topRightCornerSize, shape.bottomLeftCornerSize, shape.bottomRightCornerSize)
            .forEach { assertEquals("delete shape must be a circle", delete.width / 2f, it.getCornerSize(bounds), 0.5f) }
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

    private fun tomorrowAtTen(): Long {
        val zone = ZoneId.of("Asia/Almaty")
        val tomorrow = LocalDate.now(zone).plusDays(1)
        return LocalDateTime.of(tomorrow, LocalTime.of(10, 0)).atZone(zone).toEpochSecond()
    }

    private fun appliedFontScale(binding: ItemAlarmBinding): Float =
        binding.root.resources.configuration.fontScale

    private fun dp(view: View, id: Int): Int = view.resources.getDimensionPixelSize(id)

    private fun rippleGap(view: View): Int =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, RIPPLE_GAP_DP, view.resources.displayMetrics).toInt()

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
        const val RIPPLE_SETTLE_MS = 200L
        const val RGB_MASK = 0x00FFFFFF
    }
}

private const val RIPPLE_GAP_DP = 8f

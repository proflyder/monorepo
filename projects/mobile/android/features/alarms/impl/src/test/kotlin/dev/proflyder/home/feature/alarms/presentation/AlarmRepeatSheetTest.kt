package dev.proflyder.home.feature.alarms.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import androidx.lifecycle.SavedStateHandle
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.SheetAlarmRepeatBinding
import java.io.File
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-rRU-w360dp-h800dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlarmRepeatSheetTest {
    @Test
    fun presetSheetShowsTheListAndHidesTheDays() {
        val binding = inflate()
        bind(binding, RepeatKind.WEEKDAYS, emptySet())
        measure(binding.root)
        assertEquals(View.GONE, binding.repeatDaysGroup.visibility)
        assertEquals(true, binding.repeatOptionWeekdays.isChecked)
        savePng(binding.root, "alarm-repeat-sheet-360dp-ru.png")
    }

    @Test
    fun customSheetShowsSevenEqualDayCirclesAt360dp() {
        val binding = customBinding()
        assertDayToggles(binding)
        savePng(binding.root, "alarm-repeat-sheet-days-360dp-ru.png")
    }

    @Test
    fun customSheetKeepsCirclesAndLabelsAt360dpWithLargeFont() {
        RuntimeEnvironment.setFontScale(1.15f)
        val binding = customBinding()
        assertDayToggles(binding)
    }

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun customSheetKeepsCirclesAndLabelsAt411dp() {
        RuntimeEnvironment.setFontScale(1.15f)
        val binding = customBinding()
        assertDayToggles(binding)
    }

    @Test
    fun doneStaysDisabledWhileCustomHasNoDays() {
        val viewModel = AlarmRepeatSheetViewModel(SavedStateHandle())
        viewModel.dispatch(AlarmRepeatIntent.Choose(RepeatKind.CUSTOM))
        assertFalse(viewModel.state.value.canConfirm)
        viewModel.dispatch(AlarmRepeatIntent.SetDay(1, true))
        assertTrue(viewModel.state.value.canConfirm)
        viewModel.dispatch(AlarmRepeatIntent.SetDay(1, false))
        assertFalse(viewModel.state.value.canConfirm)
    }

    @Test
    fun savedStateRestoresTheOpenChoice() {
        val handle = SavedStateHandle()
        val first = AlarmRepeatSheetViewModel(handle)
        first.dispatch(AlarmRepeatIntent.Choose(RepeatKind.CUSTOM))
        first.dispatch(AlarmRepeatIntent.SetDay(1, true))
        first.dispatch(AlarmRepeatIntent.SetDay(3, true))

        val second = AlarmRepeatSheetViewModel(handle)
        assertEquals(AlarmRepeatChoice(RepeatKind.CUSTOM, setOf(1, 3)), second.state.value.choice)
    }

    @Test
    fun customDaysMatchingAPresetBecomeThatPreset() {
        val viewModel = AlarmRepeatSheetViewModel(SavedStateHandle())
        viewModel.dispatch(AlarmRepeatIntent.Choose(RepeatKind.CUSTOM))
        listOf(6, 7).forEach { viewModel.dispatch(AlarmRepeatIntent.SetDay(it, true)) }
        assertEquals(RepeatKind.WEEKENDS, viewModel.state.value.choice.normalized().repeat)
    }

    private fun customBinding(): SheetAlarmRepeatBinding {
        val binding = inflate()
        bind(binding, RepeatKind.CUSTOM, setOf(1, 3, 5))
        measure(binding.root)
        return binding
    }

    private fun bind(binding: SheetAlarmRepeatBinding, repeat: RepeatKind, days: Set<Int>) {
        val option = when (repeat) {
            RepeatKind.ONCE -> binding.repeatOptionOnce
            RepeatKind.DAILY -> binding.repeatOptionDaily
            RepeatKind.WEEKDAYS -> binding.repeatOptionWeekdays
            RepeatKind.WEEKENDS -> binding.repeatOptionWeekends
            else -> binding.repeatOptionCustom
        }
        option.isChecked = true
        binding.repeatDaysGroup.visibility = when (repeat) {
            RepeatKind.CUSTOM -> View.VISIBLE
            else -> View.GONE
        }
        dayButtons(binding).forEachIndexed { index, button -> button.isChecked = days.contains(index + 1) }
    }

    private fun assertDayToggles(binding: SheetAlarmRepeatBinding) {
        val buttons = dayButtons(binding)
        val expected = binding.root.resources.getStringArray(R.array.alarms_day_short)
        assertEquals(7, buttons.size)
        assertEquals(expected.toList(), buttons.map { it.text.toString() })

        val minimum = dp(binding.root, 40)
        val maximum = dp(binding.root, 44)
        val size = buttons.first().width
        assertTrue("day circle is ${dpDp(binding.root, size)}dp, expected 40..44dp", size in minimum..maximum)
        buttons.forEach { button ->
            assertEquals("day circle width differs", size, button.width)
            assertEquals("day circle height differs", size, button.height)
            assertEquals("day label is clipped or wrapped", 1, button.lineCount)
            assertTrue(
                "day label ${button.text} does not fit into the circle",
                button.layout.getLineWidth(0) <= button.width,
            )
        }
        assertEvenlySpacedCentres(binding)
    }

    private fun assertEvenlySpacedCentres(binding: SheetAlarmRepeatBinding) {
        val buttons = dayButtons(binding)
        val centres = buttons.map { button ->
            val (left, _) = offsetInRoot(binding.root, button)
            left + button.width / 2
        }
        val step = centres[1] - centres[0]
        centres.zipWithNext().forEach { (first, second) ->
            assertTrue("day spacing is uneven: ${second - first} vs $step", abs(second - first - step) <= 1)
        }
        assertTrue("the first day circle starts outside the sheet", centres.first() - buttons.first().width / 2 >= 0)
        assertTrue(
            "the last day circle ends outside the sheet",
            centres.last() + buttons.last().width / 2 <= binding.root.width,
        )
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

    private fun dayButtons(binding: SheetAlarmRepeatBinding): List<MaterialButton> = listOf(
        binding.repeatDay1,
        binding.repeatDay2,
        binding.repeatDay3,
        binding.repeatDay4,
        binding.repeatDay5,
        binding.repeatDay6,
        binding.repeatDay7,
    )

    private fun inflate(): SheetAlarmRepeatBinding {
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
        )
        return SheetAlarmRepeatBinding.inflate(LayoutInflater.from(themed))
    }

    private fun measure(root: View) {
        val width = root.resources.displayMetrics.widthPixels
        root.measure(exactly(width), unspecified())
        root.layout(0, 0, width, root.measuredHeight)
    }

    private fun savePng(root: View, name: String) {
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun dp(view: View, value: Int): Int =
        (value * view.resources.displayMetrics.density).toInt()

    private fun dpDp(view: View, value: Int): Float = value / view.resources.displayMetrics.density

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private fun unspecified(): Int = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED)
}

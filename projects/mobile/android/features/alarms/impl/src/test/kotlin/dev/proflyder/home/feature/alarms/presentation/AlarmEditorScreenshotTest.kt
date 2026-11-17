package dev.proflyder.home.feature.alarms.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmEditorBinding
import java.io.File
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The label is optional: in the dark theme the empty field must show the
 * localized default ("Alarm") as its placeholder. The new layout is also a
 * contract: Save is pinned below the scrolling content, so it is reachable
 * without scrolling even on a small 360x640dp phone. PNGs of the real layout are
 * written to Bazel's undeclared outputs as review evidence for the pull request.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlarmEditorScreenshotTest {
    @Test
    fun emptyLabelShowsDefaultPlaceholderInDarkTheme() {
        val binding = inflate()
        val labelField = binding.alarmEditorLabelLayout
        assertEquals(themed().getString(R.string.alarms_label_default), labelField.hint)
        configureNew(binding)
        savePng(binding.root, "alarm-editor-new-dark.png", 411, 891)
    }

    @Test
    fun editingShowsTheSelectedRepeatAndCustomDays() {
        val binding = inflate()
        binding.alarmEditorLabel.setText(themed().getString(R.string.alarms_label_default))
        binding.alarmEditorTime.text = String.format(Locale.ROOT, "%02d:%02d", 6, 15)
        binding.alarmEditorRepeatGroup.check(R.id.repeat_custom)
        listOf(binding.day1, binding.day3, binding.day5).forEach { it.isChecked = true }
        binding.alarmEditorDaysGroup.visibility = View.VISIBLE
        binding.alarmEditorEnabled.isChecked = true
        binding.alarmEditorDelete.visibility = View.VISIBLE
        savePng(binding.root, "alarm-editor-edit-dark.png", 411, 891)
    }

    @Test
    fun saveIsVisibleWithoutScrollingOnA360x640Phone() {
        val binding = inflate()
        configureNew(binding)
        val metrics = binding.root.resources.displayMetrics
        val width = (360 * metrics.density).toInt()
        val height = (640 * metrics.density).toInt()
        measure(binding.root, width, height)

        val save = binding.alarmEditorSave
        val bar = binding.alarmEditorSaveBar
        assertEquals(View.VISIBLE, save.visibility)
        assertTrue("bar is hidden", bar.height > 0)
        assertEquals("bar is not pinned to the bottom", binding.root.height, bar.bottom)
        val saveTop = bar.top + save.top
        val saveBottom = bar.top + save.bottom
        assertTrue("Save starts above the screen: $saveTop", saveTop >= bar.top)
        assertTrue(
            "Save is below the fold: $saveBottom > ${binding.root.height}",
            saveBottom <= binding.root.height,
        )
        assertTrue("content has no room under the bar", bar.top > 0)
    }

    private fun configureNew(binding: FragmentAlarmEditorBinding) {
        binding.alarmEditorTime.text = String.format(Locale.ROOT, "%02d:%02d", 7, 30)
        binding.alarmEditorRepeatGroup.check(R.id.repeat_daily)
        binding.alarmEditorEnabled.isChecked = true
    }

    private fun inflate(): FragmentAlarmEditorBinding =
        FragmentAlarmEditorBinding.inflate(LayoutInflater.from(themed()))

    private fun themed(): ContextThemeWrapper = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
    )

    private fun savePng(root: View, name: String, widthDp: Int, heightDp: Int) {
        val metrics = root.resources.displayMetrics
        val width = (widthDp * metrics.density).toInt()
        val height = (heightDp * metrics.density).toInt()
        measure(root, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun measure(root: View, width: Int, height: Int) {
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)
}

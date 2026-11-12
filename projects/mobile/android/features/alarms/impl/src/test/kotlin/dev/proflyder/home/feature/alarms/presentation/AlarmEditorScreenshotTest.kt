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
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The label is optional: in the dark theme the empty field must show the
 * localized default ("Alarm") as its placeholder. The PNG of the real layout is
 * written to Bazel's undeclared outputs as review evidence for the pull request.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlarmEditorScreenshotTest {
    @Test
    fun emptyLabelShowsDefaultPlaceholderInDarkTheme() {
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
        )
        val binding = FragmentAlarmEditorBinding.inflate(LayoutInflater.from(themed))
        val labelField = binding.alarmEditorLabelLayout
        assertEquals(themed.getString(R.string.alarms_label_default), labelField.hint)
        binding.alarmEditorTime.text = String.format(Locale.ROOT, "%02d:%02d", 7, 30)
        binding.alarmEditorRepeatGroup.check(R.id.repeat_daily)
        binding.alarmEditorEnabled.isChecked = true
        savePng(binding.root, "alarm-editor-dark.png")
    }

    private fun savePng(root: View, name: String) {
        val metrics = root.resources.displayMetrics
        val width = (411 * metrics.density).toInt()
        val height = (891 * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)
}

package dev.proflyder.home.feature.alarms.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding
import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * On a narrow phone the three card actions no longer fit on one line. The
 * delete button must stay inside the card: the card clips its children to the
 * rounded outline, so an overflowing button loses part of its ripple (and
 * label) at the card edge.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-rRU-w360dp-h800dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlarmItemLayoutTest {
    @Test
    fun deleteButtonStaysInsideTheNarrowCard() {
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
        )
        val binding = ItemAlarmBinding.inflate(LayoutInflater.from(themed))
        val margin = themed.resources.getDimensionPixelSize(R.dimen.alarms_screen_margin)
        val width = themed.resources.displayMetrics.widthPixels - margin * 2
        binding.root.measure(
            exactly(width),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        binding.root.layout(0, 0, width, binding.root.measuredHeight)

        assertTrue("delete right ${binding.alarmDelete.right} > card $width", binding.alarmDelete.right <= width)
        assertTrue(
            "delete bottom ${binding.alarmDelete.bottom} > card ${binding.root.height}",
            binding.alarmDelete.bottom <= binding.root.height,
        )
        savePng(binding.root, "alarm-item-narrow-ru.png")
    }

    private fun savePng(root: View, name: String) {
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)
}

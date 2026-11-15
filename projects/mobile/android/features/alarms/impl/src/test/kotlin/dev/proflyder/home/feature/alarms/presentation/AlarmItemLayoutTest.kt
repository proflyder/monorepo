package dev.proflyder.home.feature.alarms.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The card keeps a single actions row on every supported phone. Splitting the
 * row (the #325 Flow wrap) made the card twice as tall on a real 393dp phone,
 * so the row must not wrap at 360dp either, even with a 1.15 font scale. The
 * delete control is an icon button: the card clips children to its 24dp rounded
 * outline, so its circular ripple must stay at least one radius inside the edge.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-rRU-w360dp-h800dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlarmItemLayoutTest {
    @Test
    fun actionsStayOnOneLineAt360dp() = assertSingleActionsRow(1f, "alarm-item-360dp-ru.png")

    @Test
    fun actionsStayOnOneLineAt360dpWithLargeFont() = assertSingleActionsRow(1.15f, null)

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun actionsStayOnOneLineAt411dp() = assertSingleActionsRow(1f, "alarm-item-411dp-ru.png")

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun actionsStayOnOneLineAt411dpWithLargeFont() = assertSingleActionsRow(1.15f, null)

    private fun assertSingleActionsRow(fontScale: Float, pngName: String?) {
        val binding = inflate(fontScale)
        val cardWidth = cardWidth(binding.root)
        binding.root.measure(exactly(cardWidth), unspecified())
        binding.root.layout(0, 0, cardWidth, binding.root.measuredHeight)

        // Guards the large-font case against a silently ignored scale.
        assertEquals(
            "font scale not applied to the inflated card",
            fontScale,
            binding.root.resources.configuration.fontScale,
            0.001f,
        )
        val actions = listOf(binding.alarmSnooze, binding.alarmStop, binding.alarmDelete)
        val tops = actions.map { it.top }.toSet()
        assertEquals("actions split into rows: $tops", 1, tops.size)
        assertEquals("actions row is not one button high", binding.alarmDelete.height, binding.alarmActions.height)
        assertTrue(
            "stop overlaps delete: ${binding.alarmStop.right} > ${binding.alarmDelete.left}",
            binding.alarmStop.right <= binding.alarmDelete.left,
        )
        assertDeleteRippleInsideCard(binding, cardWidth)
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

    private fun inflate(fontScale: Float): ItemAlarmBinding {
        RuntimeEnvironment.setFontScale(fontScale)
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
        )
        return ItemAlarmBinding.inflate(LayoutInflater.from(themed))
    }

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
}

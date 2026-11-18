package dev.proflyder.home.feature.alarms.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsBinding
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding
import java.io.File
import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The contextual bar puts each action's label under its icon. At 360dp with a
 * 1.15 font scale, and at 411dp, every visible label must still fit on one
 * line: the old wide buttons broke "Включить" and "Удалить" mid-word.
 * The rendered PNGs show the whole selection screen for the pull request.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-rRU-w360dp-h800dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlarmSelectionBarTest {
    @Test
    fun actionLabelsStayOnOneLineAt360dp() = assertSelectionBar(1f, "alarm-selection-360dp-ru.png")

    @Test
    fun actionLabelsStayOnOneLineAt360dpWithLargeFont() = assertSelectionBar(1.15f, null)

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun actionLabelsStayOnOneLineAt411dp() = assertSelectionBar(1f, "alarm-selection-411dp-ru.png")

    @Test
    @Config(qualifiers = "ru-rRU-w411dp-h891dp-night-xxhdpi")
    fun actionLabelsStayOnOneLineAt411dpWithLargeFont() = assertSelectionBar(1.15f, null)

    private fun assertSelectionBar(fontScale: Float, pngName: String?) {
        RuntimeEnvironment.setFontScale(fontScale)
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
        )
        val binding = FragmentAlarmsBinding.inflate(LayoutInflater.from(themed))
        binding.alarmsSelectionBar.visibility = View.VISIBLE
        binding.alarmsActionBar.visibility = View.VISIBLE
        binding.alarmsAdd.visibility = View.GONE
        val count = 2
        binding.alarmsSelectionCount.text =
            themed.resources.getQuantityString(R.plurals.alarms_selected_count, count, count)
        binding.alarmsSelectionEnable.visibility = View.VISIBLE
        binding.alarmsSelectionDisable.visibility = View.VISIBLE
        binding.alarmsSelectionDelete.visibility = View.VISIBLE

        addSelectionCards(binding, themed)
        val metrics = themed.resources.displayMetrics
        binding.root.measure(exactly(metrics.widthPixels), exactly(metrics.heightPixels))
        binding.root.layout(0, 0, metrics.widthPixels, metrics.heightPixels)

        assertEquals("font scale not applied", fontScale, binding.root.resources.configuration.fontScale, 0.001f)
        assertOneLineLabel(binding.alarmsSelectionEnable, metrics.widthPixels)
        assertOneLineLabel(binding.alarmsSelectionDisable, metrics.widthPixels)
        assertOneLineLabel(binding.alarmsSelectionDelete, metrics.widthPixels)
        assertEquals(
            "selection delete action must stay on the error color",
            MaterialColors.getColor(binding.alarmsSelectionDelete, com.google.android.material.R.attr.colorError),
            binding.alarmsSelectionDelete.currentTextColor,
        )

        when (pngName) {
            null -> Unit
            else -> savePng(binding.root, pngName)
        }
    }

    /** Two picked rows: one checked, one not, to show both indicator states. */
    private fun addSelectionCards(binding: FragmentAlarmsBinding, themed: ContextThemeWrapper) {
        val metrics = themed.resources.displayMetrics
        val margin = themed.resources.getDimensionPixelSize(R.dimen.alarms_screen_margin)
        val cardWidth = metrics.widthPixels - margin * 2
        val cardGap = (12 * metrics.density).toInt()
        // Clear the 56dp selection header and leave a 12dp gap under it.
        val top = binding.alarmsSelectionBar.layoutParams.height +
            (12 * themed.resources.displayMetrics.density).toInt()
        val container = LinearLayout(themed).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            ).apply {
                leftMargin = margin
                rightMargin = margin
                topMargin = top
            }
        }
        // A RecyclerView gives a row card an unbounded height; measure it the
        // same way instead of letting the scrim's match_parent child fill the
        // screen, then pin the measured height inside the column.
        listOf(
            selectionCard(themed, "Работа", selected = true),
            selectionCard(themed, "Зарядка", selected = false),
        ).forEach { card ->
            card.measure(exactly(cardWidth), unspecified())
            card.layout(0, 0, cardWidth, card.measuredHeight)
            container.addView(
                card,
                LinearLayout.LayoutParams(cardWidth, card.measuredHeight).apply { bottomMargin = cardGap },
            )
        }
        binding.root.addView(container, 1)
    }

    private fun selectionCard(themed: ContextThemeWrapper, label: String, selected: Boolean): View {
        val card = ItemAlarmBinding.inflate(LayoutInflater.from(themed))
        bindAlarmCard(
            binding = card,
            schedule = schedule(label),
            context = themed,
            nowUnix = Instant.now().epochSecond,
            actionsAllowed = false,
            selected = selected,
        )
        return card.root
    }

    private fun assertOneLineLabel(label: TextView, screenWidth: Int) {
        assertEquals("label ${label.text} must stay on one line", 1, label.maxLines)
        assertEquals("label ${label.text} wrapped: ${label.text}", 1, label.lineCount)
        assertTrue("label ${label.text} ellipsized", label.layout.getEllipsisCount(0) == 0)
        assertTrue("action overflows the screen: ${label.right} > $screenWidth", label.right <= screenWidth)
        assertNotEquals("empty label", 0, label.text.length)
    }

    private fun schedule(label: String): Schedule = Schedule(
        id = "alarm-$label",
        label = label,
        kind = ScheduleKind.ALARM,
        timezone = "Asia/Almaty",
        at = WallTime(10, 0),
        onceDate = "",
        repeat = RepeatKind.WEEKDAYS,
        weekdays = listOf(1, 2, 3, 4, 5),
        nextDueUnix = Instant.now().epochSecond + 3600,
        enabled = true,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
    )

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

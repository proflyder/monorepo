package dev.proflyder.home.feature.alarms.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsUpcomingBinding
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmsUpcomingBinding
import java.io.File
import java.time.LocalDateTime
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class UpcomingAlarmsScreenshotTest {
    private val now = LocalDateTime.of(2026, 11, 23, 21, 0).atZone(ZoneId.systemDefault()).toEpochSecond()

    @Test
    fun theHomeSectionShowsTheNextAlarmsWithTheirSwitches() = renderSection("alarms-upcoming-section-dark.png")

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-notnight-xxhdpi")
    fun theHomeSectionShowsTheNextAlarmsInTheLightTheme() = renderSection("alarms-upcoming-section-light.png")

    private fun renderSection(name: String) {
        val binding = FragmentAlarmsUpcomingBinding.inflate(LayoutInflater.from(themed()))
        val texts = UpcomingAlarmTexts(themed())
        val rows = listOf(binding.alarmsUpcomingFirst, binding.alarmsUpcomingSecond, binding.alarmsUpcomingThird)
            .map { UpcomingAlarmRow(ItemAlarmsUpcomingBinding.bind(it), texts) { } }
        val alarms = listOf(
            UpcomingAlarm(schedule("ring", hour = 21, label = "Подъём").copy(activeOccurrenceUnix = now - 30), ringing = true),
            UpcomingAlarm(schedule("work", hour = 7, label = "Работа", repeat = RepeatKind.WEEKDAYS).copy(nextDueUnix = now + 36_000), ringing = false),
        )
        binding.root.visibility = View.VISIBLE
        rows.forEachIndexed { index, row -> row.bind(alarms.getOrNull(index), now) }
        assertEquals(View.GONE, binding.alarmsUpcomingThird.visibility)
        assertTrue(ItemAlarmsUpcomingBinding.bind(binding.alarmsUpcomingFirst).alarmsUpcomingLabel.text.contains("Подъём"))
        savePng(binding.root, name, 411, 140)
        assertNotNull(binding.alarmsUpcomingFirst.foreground)
    }

    private fun themed(): ContextThemeWrapper = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
    ).apply { theme.applyStyle(dev.proflyder.home.core.theme.R.style.ThemeOverlay_Home_Neon, true) }

    private fun savePng(root: View, name: String, widthDp: Int, heightDp: Int) {
        val metrics = root.resources.displayMetrics
        val width = (widthDp * metrics.density).toInt()
        val height = (heightDp * metrics.density).toInt()
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

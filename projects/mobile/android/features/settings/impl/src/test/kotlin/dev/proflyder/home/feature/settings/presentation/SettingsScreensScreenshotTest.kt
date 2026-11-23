package dev.proflyder.home.feature.settings.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import androidx.recyclerview.widget.LinearLayoutManager
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.feature.push.NotificationHistoryEntry
import dev.proflyder.home.feature.push.NotificationHistoryType
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentNotificationHistoryBinding
import dev.proflyder.home.feature.settings.impl.databinding.SettingsToolbarBellBinding
import dev.proflyder.home.feature.settings.impl.databinding.FragmentSettingsBinding
import java.io.File
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SettingsScreensScreenshotTest {
    @Test
    fun theSettingsListShowsEveryAreaInTheDarkTheme() {
        val themed = theme()
        val binding = FragmentSettingsBinding.inflate(LayoutInflater.from(themed))
        savePng(binding.root, "settings-hub-dark.png")
    }

    @Test
    fun theHistoryListShowsTypeIconTimeAndLinkMarker() {
        val themed = theme()
        val binding = FragmentNotificationHistoryBinding.inflate(LayoutInflater.from(themed))
        binding.historyList.layoutManager = LinearLayoutManager(themed)
        binding.historyList.adapter = NotificationHistoryAdapter(NoopCallback()).apply { submitList(rows(messages(themed))) }
        binding.historyList.addHistoryDecorations()
        binding.historyCount.text = themed.getString(R.string.history_count, 3)
        binding.historyEmpty.visibility = View.GONE
        binding.historyReadAll.visibility = View.VISIBLE
        savePng(binding.root, "notification-history-dark.png")
    }

    @Test
    fun theDayHeaderStaysPinnedWhileTheGroupScrollsUnderIt() {
        val themed = theme()
        val binding = FragmentNotificationHistoryBinding.inflate(LayoutInflater.from(themed))
        binding.historyList.layoutManager = LinearLayoutManager(themed)
        val many = (1..8).flatMap { messages(themed) }.mapIndexed { index, entry -> entry.copy(id = "e$index") }
        binding.historyList.adapter = NotificationHistoryAdapter(NoopCallback()).apply { submitList(rows(many)) }
        binding.historyList.addHistoryDecorations()
        binding.historyEmpty.visibility = View.GONE
        savePng(binding.root, "notification-history-sticky-dark.png") { binding.historyList.scrollBy(0, SCROLL_PX) }
    }

    @Test
    fun historyRowsGroupEntriesByDayWithRelativeLabels() {
        val zone = ZoneOffset.UTC
        val today = LocalDate.of(2026, 11, 23)
        val at = { date: LocalDate -> date.atStartOfDay(zone).toInstant().toEpochMilli() + HOUR_MILLIS }
        val entries = listOf(today, today, today.minusDays(1), today.minusDays(5)).mapIndexed { index, date ->
            NotificationHistoryEntry(id = "e$index", type = NotificationHistoryType.NOTIFY, title = "t", body = "b", receivedAtMillis = at(date))
        }

        val rows = HistoryRows.of(entries, today, zone)

        assertEquals(
            listOf(HistoryDay.TODAY, HistoryDay.YESTERDAY, HistoryDay.EARLIER),
            rows.filterIsInstance<HistoryRow.Day>().map { it.day },
        )
        assertEquals(listOf("day", "entry", "entry", "day", "entry", "day", "entry"), rows.map { it.key.substringBefore(':') })
    }

    private fun rows(entries: List<NotificationHistoryEntry>): List<HistoryRow> =
        HistoryRows.of(entries, LocalDate.now(), ZoneId.systemDefault())

    @Test
    fun theEmptyHistoryExplainsWhatWillAppear() {
        val themed = theme()
        val binding = FragmentNotificationHistoryBinding.inflate(LayoutInflater.from(themed))
        binding.historyClear.visibility = View.GONE
        binding.historyCount.visibility = View.GONE
        binding.historyEmpty.visibility = View.VISIBLE
        savePng(binding.root, "notification-history-empty-dark.png")
    }

    @Test
    fun theBellShowsTheUnreadCountAndCapsIt() {
        val themed = theme()
        val bells = listOf(0, 3, 120).map { unread ->
            SettingsToolbarBellBinding.inflate(LayoutInflater.from(themed)).also { NotificationBell.render(it, unread) }
        }
        assertEquals(View.GONE, bells[0].settingsBellBadge.visibility)
        assertEquals("3", bells[1].settingsBellBadge.text.toString())
        assertEquals("99+", bells[2].settingsBellBadge.text.toString())
        assertEquals(themed.getString(R.string.bell_description_unread, 3), bells[1].settingsBellButton.contentDescription)
        val row = android.widget.LinearLayout(themed).apply { bells.forEach { addView(it.root) } }
        saveSmallPng(row, "toolbar-bell-dark.png")
    }

    @Test
    fun anUnreadEntryShowsADotAndABoldTitle() {
        val themed = theme()
        val binding = FragmentNotificationHistoryBinding.inflate(LayoutInflater.from(themed))
        binding.historyList.layoutManager = LinearLayoutManager(themed)
        binding.historyList.adapter = NotificationHistoryAdapter(NoopCallback()).apply { submitList(rows(messages(themed))) }
        savePng(binding.root, "notification-history-unread-dark.png")
        val unread = binding.historyList.findViewHolderForAdapterPosition(1)!!.itemView
        val read = binding.historyList.findViewHolderForAdapterPosition(binding.historyList.adapter!!.itemCount - 1)!!.itemView
        assertEquals(View.VISIBLE, unread.findViewById<View>(R.id.history_item_unread).visibility)
        assertEquals(View.GONE, read.findViewById<View>(R.id.history_item_unread).visibility)
    }

    private fun saveSmallPng(root: View, name: String) {
        root.measure(View.MeasureSpec.UNSPECIFIED, View.MeasureSpec.UNSPECIFIED)
        root.layout(0, 0, root.measuredWidth, root.measuredHeight)
        val bitmap = Bitmap.createBitmap(root.measuredWidth, root.measuredHeight, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun messages(themed: ContextThemeWrapper): List<NotificationHistoryEntry> {
        val now = System.currentTimeMillis()
        return listOf(
            NotificationHistoryEntry(
                id = "find",
                type = NotificationHistoryType.FIND,
                title = themed.getString(R.string.history_type_find),
                body = themed.getString(R.string.history_type_find),
                receivedAtMillis = now - MINUTE_MILLIS,
            ),
            NotificationHistoryEntry(
                id = "notify",
                type = NotificationHistoryType.NOTIFY,
                title = themed.getString(R.string.settings_history_title),
                body = themed.getString(R.string.history_empty_hint),
                deepLink = "alarms",
                receivedAtMillis = now - HOUR_MILLIS,
            ),
            NotificationHistoryEntry(
                id = "earlier",
                type = NotificationHistoryType.NOTIFY,
                title = themed.getString(R.string.settings_notifications_title),
                body = themed.getString(R.string.history_empty_hint),
                receivedAtMillis = now - DAY_MILLIS,
                read = true,
            ),
        )
    }

    private fun theme(): ContextThemeWrapper = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
    )

    private class NoopCallback : BaseAdapterCallback<HistoryRow>(),
        NotificationHistoryAdapterCallback

    private fun savePng(root: View, name: String, afterLayout: () -> Unit = {}) {
        val metrics = root.resources.displayMetrics
        val width = (411 * metrics.density).toInt()
        val height = (891 * metrics.density).toInt()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        afterLayout()
        root.measure(exactly(width), exactly(height))
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val MINUTE_MILLIS = 60_000L
        const val HOUR_MILLIS = 3_600_000L
        const val DAY_MILLIS = 86_400_000L
        const val SCROLL_PX = 700
    }
}

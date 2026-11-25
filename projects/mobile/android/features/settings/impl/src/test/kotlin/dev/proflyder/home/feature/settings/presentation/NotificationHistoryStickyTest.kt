package dev.proflyder.home.feature.settings.presentation

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Looper
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.ConcatAdapter
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.navigation.routeWhenAllowed
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.applyContentInsets
import dev.proflyder.home.feature.settings.impl.databinding.FragmentNotificationHistoryBinding
import java.io.File
import java.time.LocalDate
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import dev.proflyder.home.core.designsystem.R as DesignR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NotificationHistoryStickyTest {
    @Test
    fun dayHeadersPinUnderTheChromeInDark() = checkStates("dark")

    @Test
    @Config(qualifiers = "w411dp-h891dp-notnight-xxhdpi")
    fun dayHeadersPinUnderTheChromeInLight() = checkStates("light")

    private fun checkStates(mode: String) {
        val (activity, binding) = open()
        val list = binding.historyList
        val manager = list.layoutManager as StickyHeadersLayoutManager
        val header = binding.historyGlassHeader
        assertEquals("count row sits under the chrome", BAR, header.paddingTop)
        assertEquals("content starts under the screen header", header.height, list.paddingTop)
        assertEquals("first day at the top", header.height, list.getChildAt(0).top)
        assertEquals("list clears the bottom chrome", NAV, list.paddingBottom - initialBottom(binding))
        assertNull(manager.stickyHeaderView)
        shoot(binding.root, "history-sticky-$mode-rest.png")

        scrollTo(activity, list) { manager.findViewByPosition(0)?.let { it.top - list.paddingTop + it.height * 2 } ?: 0 }
        val pinned = checkNotNull(manager.stickyHeaderView) { "a day header is pinned" }
        assertEquals("pinned day sits flush under the screen header", header.height, pinned.top)
        assertEquals(0, manager.stickyHeaderPosition)
        shoot(binding.root, "history-sticky-$mode-pinned.png")

        val next = nextDayPosition(list)
        scrollTo(activity, list) { manager.findViewByPosition(next)?.let { it.top - header.height - pinned.height / 2 } ?: pinned.height }
        assertEquals("next day pushes the pinned one", checkNotNull(manager.findViewByPosition(next)).top, checkNotNull(manager.stickyHeaderView).bottom)
        shoot(binding.root, "history-sticky-$mode-push.png")
    }

    private fun nextDayPosition(list: androidx.recyclerview.widget.RecyclerView): Int =
        (1 until checkNotNull(list.adapter).itemCount).first { list.adapter?.getItemViewType(it) == list.adapter?.getItemViewType(0) }

    private fun scrollTo(activity: Activity, list: androidx.recyclerview.widget.RecyclerView, delta: () -> Int) = repeat(PASSES) {
        list.scrollBy(0, delta())
        settle(activity)
    }

    private fun open(): Pair<Activity, FragmentNotificationHistoryBinding> {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val themed = ContextThemeWrapper(activity, DesignR.style.Base_Theme_Home)
        val binding = FragmentNotificationHistoryBinding.inflate(LayoutInflater.from(themed))
        val adapter = NotificationHistoryAdapter(Callback()).apply { submitList(rows()) }
        binding.historyList.adapter = ConcatAdapter(adapter)
        binding.historyList.addHistoryDecorations()
        binding.historyList.applyContentInsets()
        binding.historyProgress.visibility = View.GONE
        binding.historyEmpty.visibility = View.GONE
        binding.historyCount.text = COUNT
        activity.setContentView(binding.root)
        settle(activity)
        ViewCompat.dispatchApplyWindowInsets(binding.root, chromeInsets())
        settle(activity)
        ViewCompat.dispatchApplyWindowInsets(binding.root, chromeInsets())
        settle(activity)
        return activity to binding
    }

    private fun chromeInsets(): WindowInsetsCompat = WindowInsetsCompat.Builder()
        .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, BAR, 0, 0))
        .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, NAV))
        .build()

    private fun initialBottom(binding: FragmentNotificationHistoryBinding): Int =
        binding.root.resources.getDimensionPixelSize(DesignR.dimen.ds_space_xl)

    private fun settle(activity: Activity) = repeat(PASSES) {
        activity.window.decorView.requestLayout()
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun shoot(root: View, name: String) {
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun rows(): List<HistoryRow> {
        val today = LocalDate.now()
        val zone = ZoneId.systemDefault()
        val entries = (0 until DAYS).flatMap { day ->
            val start = today.minusDays(day.toLong()).atTime(EVENING_HOUR, 0).atZone(zone).toEpochSecond()
            ENTRIES.mapIndexed { index, (title, body) -> entry("$day-$index", start - index * GAP_SECONDS, title, body) }
        }
        val targets = testTargets()
        return HistoryRows.of(entries, today, zone) { item -> targets.of(item.deepLink)?.routeWhenAllowed(AppAccessState.Unknown) }
    }

    private fun entry(id: String, at: Long, title: String, body: String) = NotificationHistory(
        id = id,
        type = NotificationHistoryType.NOTIFY,
        title = title,
        body = body,
        deepLink = "home://alarms",
        createdUnix = at,
    )

    private class Callback : BaseAdapterCallback<HistoryRow>(), NotificationHistoryAdapterCallback

    private companion object {
        const val BAR = 300
        const val NAV = 240
        const val DAYS = 4
        const val PASSES = 3
        const val EVENING_HOUR = 21
        const val GAP_SECONDS = 1_800L
        const val PNG_QUALITY = 100
        const val COUNT = "24 сохранено"
        val ENTRIES = listOf(
            "Секреты" to "Показ секрета: infra/backup/offsite",
            "Секреты" to "Показ секрета: ci/test",
            "Будильник" to "Подъём 07:30",
            "Секреты" to "Привязка телефона",
            "Дом" to "Лампа в спальне включена",
            "Сценарии" to "Вечер: выполнен",
        )
    }
}

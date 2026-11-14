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
import dev.proflyder.home.feature.settings.impl.databinding.FragmentSettingsBinding
import java.io.File
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The two new screens are real Material UI: only an inflated layout proves the
 * settings rows, the type icons and the relative time render in the dark theme.
 * The PNGs are written to Bazel's undeclared outputs as review evidence.
 */
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
        binding.historyList.adapter = NotificationHistoryAdapter(NoopCallback())
        (binding.historyList.adapter as NotificationHistoryAdapter).submitList(messages(themed))
        binding.historyCount.text = themed.getString(R.string.history_count, 3)
        binding.historyEmpty.visibility = View.GONE
        savePng(binding.root, "notification-history-dark.png")
    }

    @Test
    fun theEmptyHistoryExplainsWhatWillAppear() {
        val themed = theme()
        val binding = FragmentNotificationHistoryBinding.inflate(LayoutInflater.from(themed))
        binding.historyClear.visibility = View.GONE
        binding.historyCount.visibility = View.GONE
        binding.historyEmpty.visibility = View.VISIBLE
        savePng(binding.root, "notification-history-empty-dark.png")
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
                body = themed.getString(R.string.settings_history_subtitle),
                deepLink = "alarms",
                receivedAtMillis = now - HOUR_MILLIS,
            ),
            NotificationHistoryEntry(
                id = "earlier",
                type = NotificationHistoryType.NOTIFY,
                title = themed.getString(R.string.settings_notifications_title),
                body = themed.getString(R.string.history_empty_hint),
                receivedAtMillis = now - DAY_MILLIS,
            ),
        )
    }

    private fun theme(): ContextThemeWrapper = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
    )

    private class NoopCallback : BaseAdapterCallback<NotificationHistoryEntry>(),
        NotificationHistoryAdapterCallback

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

    private companion object {
        const val MINUTE_MILLIS = 60_000L
        const val HOUR_MILLIS = 3_600_000L
        const val DAY_MILLIS = 86_400_000L
    }
}

package dev.proflyder.home.feature.settings.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Looper
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import androidx.recyclerview.widget.ItemTouchHelper
import androidx.recyclerview.widget.LinearLayoutManager
import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.Pager
import dev.proflyder.home.core.data.PagerRemoval
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.feature.push.NotificationHistoryRepository
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentNotificationHistoryBinding
import java.io.File
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HistorySwipeTest {
    private val now = System.currentTimeMillis()
    private val entries = listOf(entry("a", now, read = false), entry("b", now - HOUR, read = true), entry("c", now - 2 * HOUR, read = false))

    @Test
    fun deletingKeepsTheEntryForUndoAndUndoPutsItBack() {
        val repository = FakeRepository(entries)
        val model = NotificationHistoryViewModel(repository, FakeClock(now))
        idle()

        model.dispatch(HistoryIntent.Delete(entries[1]))
        idle()
        assertEquals(listOf("a", "c"), repository.pager.state.value.items.map { it.id })
        assertEquals("b", model.state.value.undo?.id)

        model.dispatch(HistoryIntent.Undo)
        idle()
        assertEquals(listOf("a", "b", "c"), repository.pager.state.value.items.map { it.id })
        assertNull(model.state.value.undo)
    }

    @Test
    fun swipingRightTogglesReadBothWays() {
        val repository = FakeRepository(entries)
        val model = NotificationHistoryViewModel(repository, FakeClock(now))
        idle()

        model.dispatch(HistoryIntent.ToggleRead(entries[0]))
        idle()
        assertEquals(true, repository.pager.state.value.items.first { it.id == "a" }.read)
        model.dispatch(HistoryIntent.ToggleRead(entries[1]))
        idle()
        assertEquals(false, repository.pager.state.value.items.first { it.id == "b" }.read)
    }

    @Test
    fun readingAllMarksEveryEntryAndClearsTheUndoSlot() {
        val repository = FakeRepository(entries)
        val model = NotificationHistoryViewModel(repository, FakeClock(now))
        idle()

        model.dispatch(HistoryIntent.ReadAll)
        idle()

        assertEquals(0, model.state.value.unread)
        assertEquals(listOf(true, true, true), repository.pager.state.value.items.map { it.read })
    }

    @Test
    fun leftDeletesRightTogglesAndDayHeadersDoNotSwipe() {
        val themed = theme()
        val binding = list(themed)
        val calls = mutableListOf<String>()
        val swipe = HistorySwipe(Recorder(binding, calls), HistorySwipeBackground(themed))
        val header = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(0))
        val first = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(1))

        assertEquals(0, swipe.getSwipeDirs(binding.historyList, header))
        assertEquals(ItemTouchHelper.LEFT or ItemTouchHelper.RIGHT, swipe.getSwipeDirs(binding.historyList, first))
        swipe.onSwiped(first, ItemTouchHelper.LEFT)
        swipe.onSwiped(first, ItemTouchHelper.RIGHT)
        assertEquals(listOf("delete:a", "toggle:a"), calls)
    }

    @Test
    fun swipeBackgroundsUseTheActionColourAndIcon() {
        val themed = theme()
        val background = HistorySwipeBackground(themed)
        assertEquals(R.drawable.settings_ic_delete, background.sideFor(left = true, read = false).iconRes)
        assertEquals(R.drawable.settings_ic_mark_read, background.sideFor(left = false, read = false).iconRes)
        assertEquals(R.drawable.settings_ic_mark_unread, background.sideFor(left = false, read = true).iconRes)

        val binding = list(themed)
        val root = binding.root
        idle()
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        root.draw(canvas)
        swiped(binding, background, canvas, position = 1, dx = -SWIPE_PX, read = false)
        swiped(binding, background, canvas, position = 3, dx = SWIPE_PX, read = false)
        save(bitmap, "notification-history-swipe-dark.png")
        val deleteItem = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(1)).itemView
        val deleteArea = bitmap.getPixel(binding.historyList.left + deleteItem.right - EDGE_PX, centerOf(binding, 1))
        assertEquals(background.sideFor(left = true, read = false).color, deleteArea)
    }

    private fun swiped(binding: FragmentNotificationHistoryBinding, background: HistorySwipeBackground, canvas: Canvas, position: Int, dx: Float, read: Boolean) {
        val item = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(position)).itemView
        canvas.save()
        canvas.translate(binding.historyList.left.toFloat(), binding.historyList.top.toFloat() + parentTop(binding))
        background.draw(canvas, item, dx, read)
        canvas.translate(item.left + dx, item.top.toFloat())
        item.draw(canvas)
        canvas.restore()
    }

    private fun parentTop(binding: FragmentNotificationHistoryBinding): Float = (binding.historyList.parent as View).top.toFloat()

    private fun centerOf(binding: FragmentNotificationHistoryBinding, position: Int): Int {
        val item = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(position)).itemView
        return (parentTop(binding) + binding.historyList.top + item.top + item.height / 2).toInt()
    }

    private fun list(themed: ContextThemeWrapper): FragmentNotificationHistoryBinding {
        val binding = FragmentNotificationHistoryBinding.inflate(LayoutInflater.from(themed))
        binding.historyList.layoutManager = LinearLayoutManager(themed)
        binding.historyList.adapter = NotificationHistoryAdapter(NoopCallback()).apply {
            submitList(HistoryRows.of(entries, LocalDate.now(), ZoneId.systemDefault()))
        }
        binding.historyEmpty.visibility = View.GONE
        binding.historyProgress.visibility = View.GONE
        val metrics = themed.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        val height = (HEIGHT_DP * metrics.density).toInt()
        binding.root.measure(exactly(width), exactly(height))
        binding.root.layout(0, 0, width, height)
        return binding
    }

    private class Recorder(private val binding: FragmentNotificationHistoryBinding, private val calls: MutableList<String>) : HistorySwipeActions {
        override fun entryAt(position: Int): NotificationHistory? =
            ((binding.historyList.adapter as NotificationHistoryAdapter).currentList.getOrNull(position) as? HistoryRow.Item)?.entry

        override fun onDelete(entry: NotificationHistory) {
            calls += "delete:${entry.id}"
        }

        override fun onToggleRead(entry: NotificationHistory) {
            calls += "toggle:${entry.id}"
        }
    }

    private class FakeRepository(initial: List<NotificationHistory>) : NotificationHistoryRepository {
        private var all = initial
        private val scope = CoroutineScope(Dispatchers.Main)
        private val mutableUnread = MutableStateFlow(initial.count { !it.read })

        override val pager = Pager(scope, { _, _ -> Outcome.Success(Page(all, null)) }, NotificationHistory::id)

        override val unread: StateFlow<Int> = mutableUnread

        init {
            pager.refresh(RefreshMode.HARD)
        }

        override fun refresh() = pager.refresh(RefreshMode.HARD)

        override fun removeLocal(entry: NotificationHistory): PagerRemoval<NotificationHistory>? = pager.removeLocal(entry.id)

        override fun restoreLocal(removal: PagerRemoval<NotificationHistory>) = pager.restoreLocal(removal)

        override suspend fun setRead(entry: NotificationHistory, read: Boolean): Outcome<Unit> {
            all = all.map { if (it.id == entry.id) it.copy(read = read) else it }
            pager.updateLocal(entry.id) { it.copy(read = read) }
            mutableUnread.value = pager.itemsSnapshot().count { !it.read }
            return Outcome.Success(Unit)
        }

        override suspend fun remove(entry: NotificationHistory): Outcome<Unit> {
            all = all.filterNot { it.id == entry.id }
            return Outcome.Success(Unit)
        }

        override fun removeLater(entry: NotificationHistory) {
            all = all.filterNot { it.id == entry.id }
        }

        override suspend fun readAll(): Outcome<Unit> {
            all = all.map { it.copy(read = true) }
            pager.replaceLocal(pager.itemsSnapshot().map { it.copy(read = true) })
            mutableUnread.value = 0
            return Outcome.Success(Unit)
        }

        override suspend fun clear(): Outcome<Unit> {
            all = emptyList()
            pager.replaceLocal(emptyList())
            mutableUnread.value = 0
            return Outcome.Success(Unit)
        }
    }

    private class NoopCallback : BaseAdapterCallback<HistoryRow>(), NotificationHistoryAdapterCallback

    private fun entry(id: String, at: Long, read: Boolean) = NotificationHistory(
        id = id,
        type = NotificationHistoryType.NOTIFY,
        title = "Дверь открыта $id",
        body = "Кто-то у двери",
        createdUnix = at / MILLIS_PER_SECOND,
        read = read,
    )

    private fun theme(): ContextThemeWrapper = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
    )

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    private fun save(bitmap: Bitmap, name: String) {
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val HOUR = 3_600_000L
        const val MILLIS_PER_SECOND = 1_000L
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val SWIPE_PX = 360f
        const val PNG_QUALITY = 100
        const val EDGE_PX = 12
    }
}

package dev.proflyder.home.feature.settings.presentation

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Looper
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import androidx.recyclerview.widget.ItemTouchHelper
import androidx.recyclerview.widget.LinearLayoutManager
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageRequest
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedSource
import dev.proflyder.home.core.data.PagingAction
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.ListEdit
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.presentation.ListItemIntent
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.SwipeAction
import dev.proflyder.home.core.ui.SwipeActions
import dev.proflyder.home.core.ui.SwipeBackground
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
import org.junit.Assert.assertTrue
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
        val model = NotificationHistoryViewModel(repository, CoroutineScope(Dispatchers.Main))
        idle()

        model.dispatch(ListItemIntent.Remove("b"))
        idle()
        assertEquals(listOf("a", "c"), repository.paged.state.value.items.map { it.key })
        assertEquals("b", model.state.value.pendingRemoval?.key)

        model.dispatch(ListItemIntent.Undo)
        idle()
        assertEquals(listOf("a", "b", "c"), repository.paged.state.value.items.map { it.key })
        assertNull(model.state.value.pendingRemoval)
    }

    @Test
    fun swipingRightTogglesReadBothWays() {
        val repository = FakeRepository(entries)
        val model = NotificationHistoryViewModel(repository, CoroutineScope(Dispatchers.Main))
        idle()

        model.dispatch(HistoryIntent.ToggleRead("a"))
        idle()
        assertEquals(true, repository.paged.state.value.item("a")?.read)
        model.dispatch(HistoryIntent.ToggleRead("b"))
        idle()
        assertEquals(false, repository.paged.state.value.item("b")?.read)
    }

    @Test
    fun readingAllMarksEveryEntry() {
        val repository = FakeRepository(entries)
        val model = NotificationHistoryViewModel(repository, CoroutineScope(Dispatchers.Main))
        idle()

        model.dispatch(HistoryIntent.ReadAll)
        idle()

        assertTrue(repository.paged.state.value.items.all { it.read })
    }

    @Test
    fun leftDeletesRightTogglesAndDayHeadersDoNotSwipe() {
        val themed = theme()
        val binding = list(themed)
        val calls = mutableListOf<String>()
        val swipe = SwipeActions(
            themed,
            left = action<ListItemIntent>(ItemTouchHelper.LEFT, R.drawable.settings_ic_delete) { ListItemIntent.Remove(it) },
            right = action<HistoryIntent>(ItemTouchHelper.RIGHT, R.drawable.settings_ic_mark_unread) { HistoryIntent.ToggleRead(it) },
            itemAt = { position -> (binding.historyList.adapter as NotificationHistoryAdapter).currentList.getOrNull(position)?.let { (it as? HistoryRow.Item)?.entry } },
            dispatchLeft = { calls += it.toString() },
            dispatchRight = { calls += it.toString() },
        )
        val header = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(0))
        val first = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(1))

        assertEquals(0, swipe.getSwipeDirs(binding.historyList, header))
        assertEquals(ItemTouchHelper.LEFT or ItemTouchHelper.RIGHT, swipe.getSwipeDirs(binding.historyList, first))
        swipe.onSwiped(first, ItemTouchHelper.LEFT)
        swipe.onSwiped(first, ItemTouchHelper.RIGHT)
        assertEquals(0f, first.itemView.translationX)
        assertEquals(2, calls.size)
    }

    @Test
    fun theSwipeBackgroundsUseErrorAndPrimaryColours() {
        val themed = theme()
        val binding = list(themed)
        val canvas = Canvas(Bitmap.createBitmap(binding.root.width, binding.root.height, Bitmap.Config.ARGB_8888))
        val item = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(1)).itemView
        val delete = SwipeAction(ItemTouchHelper.LEFT, R.drawable.settings_ic_delete, color(themed, com.google.android.material.R.attr.colorError), color(themed, com.google.android.material.R.attr.colorOnError)) { null }
        val toggle = SwipeAction(ItemTouchHelper.RIGHT, R.drawable.settings_ic_mark_unread, color(themed, androidx.appcompat.R.attr.colorPrimary), color(themed, com.google.android.material.R.attr.colorOnPrimary)) { null }
        val background = SwipeBackground(themed)

        background.draw(canvas, item, -SWIPE_PX, delete)
        background.draw(canvas, item, SWIPE_PX, toggle)

        assertEquals(delete.color, color(themed, com.google.android.material.R.attr.colorError))
        assertEquals(toggle.color, color(themed, androidx.appcompat.R.attr.colorPrimary))
    }

    @Test
    fun swipePngsShowTheActionAndNoPlaqueAfterDeletion() {
        val themed = theme()
        val binding = list(themed)
        val item = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(1)).itemView
        val delete = SwipeAction(ItemTouchHelper.LEFT, R.drawable.settings_ic_delete, color(themed, com.google.android.material.R.attr.colorError), color(themed, com.google.android.material.R.attr.colorOnError)) { null }
        val toggle = SwipeAction(ItemTouchHelper.RIGHT, R.drawable.settings_ic_mark_unread, color(themed, androidx.appcompat.R.attr.colorPrimary), color(themed, com.google.android.material.R.attr.colorOnPrimary)) { null }
        val background = SwipeBackground(themed)

        save(swipedRow(item, background, -SWIPE_PX, delete), "notification-history-swipe-left.png")
        save(swipedRow(item, background, SWIPE_PX, toggle), "notification-history-swipe-right.png")

        val filtered = entries.filterNot { it.key == "a" }
        binding.historyList.adapter = NotificationHistoryAdapter(NoopCallback()).apply {
            submitList(HistoryRows.of(filtered, LocalDate.now(), ZoneId.systemDefault()))
        }
        idle()
        binding.root.measure(exactly(binding.root.width), exactly(binding.root.height))
        binding.root.layout(0, 0, binding.root.width, binding.root.height)
        save(bitmap(binding.root), "notification-history-after-delete.png")
    }

    private fun swipedRow(item: View, background: SwipeBackground, dx: Float, action: SwipeAction<*>): Bitmap {
        val bitmap = Bitmap.createBitmap(item.width, item.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        canvas.translate(-item.left.toFloat(), -item.top.toFloat())
        item.translationX = 0f
        background.draw(canvas, item, dx, action)
        canvas.translate(dx, 0f)
        item.draw(canvas)
        return bitmap
    }

    private fun bitmap(view: View): Bitmap {
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        view.draw(Canvas(bitmap))
        return bitmap
    }

    private fun <T> action(
        direction: Int,
        icon: Int,
        intent: (String) -> T,
    ) = SwipeAction(direction, icon, 0, 0) { position ->
        position.takeIf { it > 0 }?.let { intent("entry") }
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

    private class FakeRepository(initial: List<NotificationHistory>) : NotificationHistoryRepository {
        private var all = initial
        private val scope = CoroutineScope(Dispatchers.Main)
        private val mutableUnread = MutableStateFlow(initial.count { !it.read })

        override val paged: PagedList<NotificationHistory> = PagedList(scope, object : PagedSource<NotificationHistory> {
            override suspend fun load(request: PageRequest): Outcome<Page<NotificationHistory>> =
                Outcome.Success(Page(all, null))
        })

        override suspend fun load(request: PageRequest): Outcome<Page<NotificationHistory>> =
            Outcome.Success(Page(all, null))

        override val unread: StateFlow<Int> = mutableUnread

        init {
            paged.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        }

        override suspend fun setRead(entry: NotificationHistory, read: Boolean): Outcome<Unit> {
            all = all.map { if (it.key == entry.key) it.copy(read = read) else it }
            mutableUnread.value = all.count { !it.read }
            return paged.commit(ListEdit.Update(entry.key) { it.copy(read = read) }) { Outcome.Success(Unit) }
        }

        override suspend fun remove(entry: NotificationHistory): Outcome<Unit> {
            all = all.filterNot { it.key == entry.key }
            mutableUnread.value = all.count { !it.read }
            return paged.commit(ListEdit.Remove(entry.key)) { Outcome.Success(Unit) }
        }

        override suspend fun readAll(): Outcome<Unit> {
            all = all.map { it.copy(read = true) }
            mutableUnread.value = 0
            return paged.commit(ListEdit.UpdateAll { it.copy(read = true) }) { Outcome.Success(Unit) }
        }

        override suspend fun clear(): Outcome<Unit> {
            all = emptyList()
            mutableUnread.value = 0
            paged.dispatch(PagingAction.Refresh(RefreshMode.HARD))
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

    private fun color(themed: ContextThemeWrapper, attr: Int): Int = MaterialColors.getColor(themed, attr, 0)

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
    }
}

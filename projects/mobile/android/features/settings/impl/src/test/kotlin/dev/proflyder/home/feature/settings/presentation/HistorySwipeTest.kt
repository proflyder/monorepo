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
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageRequest
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedSource
import dev.proflyder.home.core.data.PagingAction
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.ListEdit
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.ListItemIntent
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.SwipeActions
import dev.proflyder.home.core.ui.SwipeBackground
import dev.proflyder.home.core.ui.SwipeStyle
import dev.proflyder.home.core.ui.swipeAction
import dev.proflyder.home.feature.push.NotificationHistoryRepository
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentNotificationHistoryBinding
import java.io.File
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.After
import org.junit.Before
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
@OptIn(ExperimentalCoroutinesApi::class)
class HistorySwipeTest {
    private val dispatcher = StandardTestDispatcher()

    @Before
    fun setUp() {
        Dispatchers.setMain(dispatcher)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    private val now = System.currentTimeMillis()
    private val entries = listOf(entry("a", now, read = false), entry("b", now - HOUR, read = true), entry("c", now - 2 * HOUR, read = false))

    @Test
    fun deletingKeepsTheEntryForUndoAndUndoPutsItBack() {
        val repository = FakeRepository(entries)
        val model = model(repository)
        idle()
        assertEquals(listOf("a", "b", "c"), model.state.value.pages.items.map { it.key })

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
        val model = model(repository)
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
        val model = model(repository)
        idle()

        model.dispatch(HistoryIntent.ReadAll)
        idle()

        assertTrue(repository.paged.state.value.items.all { it.read })
        assertTrue(!model.state.value.hasUnread)
        assertTrue(model.state.value.rows.isNotEmpty())
    }

    @Test
    fun anEntryWithATargetOpensItsRouteAndMarksItRead() {
        val entry = entry("a", now, read = false, deepLink = "home://time/timers")
        val repository = FakeRepository(listOf(entry))
        val model = model(repository, FakeAccessSource(accessState(AppModule.ALARMS_TIMERS)))
        idle()

        model.dispatch(HistoryIntent.Open(entry))
        idle()
        val event = nextEvent(model)

        assertEquals(HistoryEvent.OpenRoute(Route.Tab("timers")), event)
        assertEquals(true, repository.paged.state.value.item("a")?.read)
    }

    @Test
    fun anEntryWithoutATargetIsNeitherOpenedNorMarkedRead() {
        val entry = entry("a", now, read = false)
        val repository = FakeRepository(listOf(entry))
        val model = model(repository)
        idle()

        model.dispatch(HistoryIntent.Open(entry))
        idle()
        val event = nextEvent(model)

        assertNull(event)
        assertEquals(false, repository.paged.state.value.item("a")?.read)
        assertNull(model.state.value.rows.filterIsInstance<HistoryRow.Item>().single().target)
    }

    @Test
    fun aTargetOfAModuleTheTokenLacksHasNoRoute() {
        val entry = entry("a", now, read = false, deepLink = "home://chat/message/m_1")
        val repository = FakeRepository(listOf(entry))
        val model = model(repository, FakeAccessSource(accessState(AppModule.ALARMS_TIMERS)))
        idle()

        model.dispatch(HistoryIntent.Open(entry))
        idle()
        val event = nextEvent(model)

        assertNull(event)
        assertEquals(false, repository.paged.state.value.item("a")?.read)
        assertNull(model.state.value.rows.filterIsInstance<HistoryRow.Item>().single().target)
    }

    @Test
    fun accessChangesRerenderTheRowTargets() {
        val entry = entry("a", now, read = false, deepLink = "home://chat/message/m_1")
        val access = FakeAccessSource(accessState())
        val model = model(FakeRepository(listOf(entry)), access)
        idle()
        assertNull(model.state.value.rows.filterIsInstance<HistoryRow.Item>().single().target)

        access.state.value = accessState(AppModule.CHAT)
        idle()

        assertEquals(Route.Link("chat/message/m_1"), model.state.value.rows.filterIsInstance<HistoryRow.Item>().single().target)
    }

    @Test
    fun leftDeletesRightTogglesAndDayHeadersDoNotSwipe() {
        val themed = theme()
        val binding = list(themed)
        val calls = mutableListOf<String>()
        val swipe = SwipeActions(
            themed,
            left = swipeAction(ItemTouchHelper.LEFT, SwipeStyle.Delete) { position -> calls += "left:$position" },
            right = swipeAction(ItemTouchHelper.RIGHT, SwipeStyle.ToggleRead) { position -> calls += "right:$position" },
            itemAt = { position -> (binding.historyList.adapter as NotificationHistoryAdapter).currentList.getOrNull(position)?.let { (it as? HistoryRow.Item)?.entry } },
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
        val delete = swipeAction(ItemTouchHelper.LEFT, SwipeStyle.Delete) { }
        val toggle = swipeAction(ItemTouchHelper.RIGHT, SwipeStyle.ToggleRead) { }
        val background = SwipeBackground(themed)

        background.draw(canvas, item, -SWIPE_PX, delete)
        background.draw(canvas, item, SWIPE_PX, toggle)

        assertEquals(color(themed, com.google.android.material.R.attr.colorError), color(themed, SwipeStyle.Delete.colorAttr))
        assertEquals(color(themed, androidx.appcompat.R.attr.colorPrimary), color(themed, SwipeStyle.ToggleRead.colorAttr))
    }

    @Test
    fun swipePngsShowTheActionAndNoPlaqueAfterDeletion() {
        val themed = theme()
        val binding = list(themed)
        val item = checkNotNull(binding.historyList.findViewHolderForAdapterPosition(1)).itemView
        val delete = swipeAction(ItemTouchHelper.LEFT, SwipeStyle.Delete) { }
        val toggle = swipeAction(ItemTouchHelper.RIGHT, SwipeStyle.ToggleRead) { }
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

    private fun swipedRow(item: View, background: SwipeBackground, dx: Float, action: dev.proflyder.home.core.ui.SwipeAction): Bitmap {
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

    private fun model(
        repository: NotificationHistoryRepository,
        access: FakeAccessSource = FakeAccessSource(),
    ): NotificationHistoryViewModel =
        NotificationHistoryViewModel(repository, testTargets(), access, FakeClock(now), CoroutineScope(Dispatchers.Main))

    private fun nextEvent(model: NotificationHistoryViewModel): HistoryEvent? =
        runBlocking { withTimeoutOrNull(EVENT_TIMEOUT_MS) { model.events.first() } }

    private fun accessState(vararg modules: AppModule): AppAccessState =
        AppAccessState.Ready(AppAccess(name = "test", modules = modules.toSet(), owner = false))

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

    private fun entry(id: String, at: Long, read: Boolean, deepLink: String? = null) = NotificationHistory(
        id = id,
        type = NotificationHistoryType.NOTIFY,
        title = "Дверь открыта $id",
        body = "Кто-то у двери",
        deepLink = deepLink,
        createdUnix = at / MILLIS_PER_SECOND,
        read = read,
    )

    private fun theme(): ContextThemeWrapper = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_Dark_NoActionBar,
    )

    private fun color(themed: ContextThemeWrapper, attr: Int): Int = MaterialColors.getColor(themed, attr, 0)

    private fun idle() {
        dispatcher.scheduler.runCurrent()
        shadowOf(Looper.getMainLooper()).idle()
    }

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
        const val EVENT_TIMEOUT_MS = 50L
    }
}

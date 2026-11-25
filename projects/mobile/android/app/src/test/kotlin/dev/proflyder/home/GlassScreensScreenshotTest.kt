package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.annotation.IdRes
import androidx.annotation.LayoutRes
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.glass.GlassChrome
import dev.proflyder.home.core.designsystem.glass.GlassScreenHeader
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.Robolectric
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import dev.proflyder.home.feature.control.impl.R as ControlR
import dev.proflyder.home.feature.secrets.impl.R as SecretsR
import dev.proflyder.home.feature.settings.impl.R as SettingsR

@RunWith(ParameterizedRobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = GLASS_DARK)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class GlassScreensScreenshotTest(private val screen: GlassScreen) {
    @Test
    fun dark() = shootStates("dark")

    @Test
    @Config(qualifiers = GLASS_LIGHT)
    fun light() = shootStates("light")

    private fun shootStates(mode: String) {
        val activity = launch()
        val chrome = activity.glassChrome
        val list = screen.list?.let { activity.findViewById<RecyclerView>(it) }
        if (list == null) {
            shoot(activity, "${screen.name}-$mode-rest.png")
            assertEquals(chrome.barHeight, chrome.edgeBottom)
            return
        }
        seed(activity, list)
        shootRest(activity, list, mode)
        shootPinned(activity, list, mode)
        shootPush(activity, list, mode)
    }

    private fun shootRest(activity: MainActivity, list: RecyclerView, mode: String) {
        shoot(activity, "${screen.name}-$mode-rest.png")
        val chrome = activity.glassChrome
        assertNull("${screen.name}: nothing pinned at rest", manager(list).stickyHeaderView)
        assertEquals("${screen.name}: first item under the glass", list.paddingTop, list.getChildAt(0).outerTop())
        assertTrue("${screen.name}: content starts below the glass", list.windowTop() + list.paddingTop >= chrome.glassBottom)
    }

    private fun shootPinned(activity: MainActivity, list: RecyclerView, mode: String) {
        scrollUntil(activity, list) { edge -> headerTop(list, 0)?.let { it - edge + rowHeight(list) } ?: 0 }
        shoot(activity, "${screen.name}-$mode-pinned.png")
        val pinned = checkNotNull(manager(list).stickyHeaderView) { "${screen.name}: header pinned" }
        assertEquals("${screen.name}: pinned header flush under the glass", activity.glassChrome.glassBottom, list.windowTop() + pinned.top)
        assertEquals("${screen.name}: single edge under the pinned header", list.windowTop() + pinned.bottom, activity.glassChrome.edgeBottom)
    }

    private fun shootPush(activity: MainActivity, list: RecyclerView, mode: String) {
        val section = TestSectionedRows.SECTION_SIZE
        scrollUntil(activity, list) { edge ->
            val next = headerTop(list, section) ?: return@scrollUntil rowHeight(list)
            next - edge - stickyHeight(list) / 2
        }
        shoot(activity, "${screen.name}-$mode-push.png")
        val pinned = checkNotNull(manager(list).stickyHeaderView) { "${screen.name}: header pinned" }
        val next = checkNotNull(headerTop(list, section))
        assertEquals("${screen.name}: previous header still pinned", 0, manager(list).stickyHeaderPosition)
        assertEquals("${screen.name}: next header pushes the pinned one", next, pinned.bottom)
    }

    private fun scrollUntil(activity: MainActivity, list: RecyclerView, delta: (Int) -> Int) = repeat(SCROLL_PASSES) {
        val dy = delta(edge(activity.glassChrome, list))
        if (dy != 0) list.scrollBy(0, dy)
        settle(activity)
    }

    private fun edge(chrome: GlassChrome, list: RecyclerView): Int = chrome.glassBottom - list.windowTop()

    private fun headerTop(list: RecyclerView, position: Int): Int? =
        manager(list).findViewByPosition(position)?.takeIf { it !== manager(list).stickyHeaderView }?.outerTop()

    private fun rowHeight(list: RecyclerView): Int = checkNotNull(manager(list).findViewByPosition(TestSectionedRows.SECTION_SIZE - 1)).height

    private fun stickyHeight(list: RecyclerView): Int = manager(list).stickyHeaderView?.height ?: 0

    private fun manager(list: RecyclerView) = list.layoutManager as StickyHeadersLayoutManager

    private fun seed(activity: MainActivity, list: RecyclerView) {
        list.adapter = TestSectionedRows(activity, screen.headerLayout, screen.rows) { view, section -> screen.bindHeader(view, section) }
        revealContent(list)
        settle(activity)
        list.scrollToPosition(0)
        settle(activity)
    }

    private fun revealContent(list: View) {
        var child = list
        while (true) {
            child.visibility = View.VISIBLE
            val parent = child.parent as? ViewGroup ?: return
            if (parent.id == R.id.fragment_container) return
            (0 until parent.childCount).map(parent::getChildAt)
                .filter { it !== child && it !is GlassScreenHeader }
                .forEach { it.visibility = View.GONE }
            child = parent
        }
    }

    private fun launch(): MainActivity {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        runBlocking { app.graph.appearance.selectTheme(AppTheme.EMERALD) }
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        settle(activity)
        screen.open(activity)
        settle(activity)
        applyInsets(activity)
        return activity
    }

    private fun applyInsets(activity: MainActivity) {
        val root = activity.window.decorView
        val insets = WindowInsetsCompat.Builder()
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, dp(root, STATUS_BAR_DP), 0, 0))
            .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, dp(root, NAV_BAR_DP)))
            .build()
        ViewCompat.dispatchApplyWindowInsets(root, insets)
        settle(activity)
    }

    private fun settle(activity: MainActivity) = repeat(SETTLE_PASSES) { settleOnce(activity) }

    private fun settleOnce(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        activity.window.decorView.requestLayout()
        shadowOf(activity.mainLooper).idle()
    }

    private fun shoot(activity: MainActivity, name: String) {
        val root = activity.window.decorView
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        System.getenv("TEST_UNDECLARED_OUTPUTS_DIR")?.let { dir ->
            File(dir).mkdirs()
            File(dir, "glass-$name").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
        }
    }

    class GlassScreen(
        val name: String,
        @IdRes val list: Int? = null,
        @LayoutRes val headerLayout: Int = 0,
        val rows: List<String> = emptyList(),
        val bindHeader: (View, Int) -> Unit = { _, _ -> },
        val open: (MainActivity) -> Unit,
    ) {
        override fun toString() = name
    }

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun screens(): List<Array<Any>> = listOf(HOME, HISTORY, SECRETS, PLAIN).map { arrayOf(it) }

        private val HOME = GlassScreen(
            name = "home",
            list = ControlR.id.control_list,
            headerLayout = ControlR.layout.item_control_header,
            rows = listOf("Лампа · Вкл · 100%", "Светильник · Вкл · 60%", "Комп · Выкл", "Лента · Вкл · 100%", "Колонка · Громкость 5%"),
            bindHeader = ::bindRoom,
        ) { it.router.selectTab("control") }

        private val HISTORY = GlassScreen(
            name = "history",
            list = SettingsR.id.history_list,
            headerLayout = SettingsR.layout.item_history_day,
            rows = listOf(
                "Секреты · Показ секрета: infra/backup/offsite",
                "Секреты · Показ секрета: ci/test",
                "Будильник · Подъём 07:30",
                "Секреты · Привязка телефона",
                "Дом · Лампа включена",
            ),
            bindHeader = titled(SettingsR.id.history_day_title, listOf("Сегодня", "Вчера", "22 ноября", "21 ноября")),
        ) { it.router.openRoute("settings/history") }

        private val SECRETS = GlassScreen(
            name = "secrets",
            list = SecretsR.id.secrets_catalog_list,
            headerLayout = SecretsR.layout.item_secret_group,
            rows = listOf("infra/backup/offsite", "infra/vpn/ocserv", "ci/test", "ci/release/signing", "k8s/nexus/admin"),
            bindHeader = titled(SecretsR.id.secret_group_title, listOf("infra", "ci", "k8s", "home")),
        ) { it.router.openRoute("secrets/catalog") }

        private val PLAIN = GlassScreen(name = "system") { it.router.selectTab("system") }

        private fun bindRoom(view: View, section: Int) {
            view.findViewById<TextView>(ControlR.id.control_header_title).text = ROOMS[section % ROOMS.size]
            view.findViewById<TextView>(ControlR.id.control_header_summary).text = ROOM_SUMMARIES[section % ROOM_SUMMARIES.size]
        }

        private fun titled(@IdRes title: Int, titles: List<String>): (View, Int) -> Unit = { view, section ->
            view.findViewById<TextView>(title).text = titles[section % titles.size]
        }

        private val ROOMS = listOf("Спальня", "Гостиная", "Кухня", "Прихожая")
        private val ROOM_SUMMARIES = listOf("Включено 5 из 8", "Включено 1 из 4", "Выключено", "Включено 2 из 3")
    }
}

private fun View.outerTop(): Int = top - ((layoutParams as? ViewGroup.MarginLayoutParams)?.topMargin ?: 0)

private fun View.windowTop(): Int = IntArray(2).also(::getLocationInWindow)[1]

private fun dp(view: View, value: Int): Int = (value * view.resources.displayMetrics.density).toInt()

private const val GLASS_DARK = "w411dp-h891dp-night-xxhdpi"
private const val GLASS_LIGHT = "w411dp-h891dp-notnight-xxhdpi"
private const val STATUS_BAR_DP = 32
private const val NAV_BAR_DP = 24
private const val SETTLE_PASSES = 3
private const val SCROLL_PASSES = 3
private const val PNG_QUALITY = 100

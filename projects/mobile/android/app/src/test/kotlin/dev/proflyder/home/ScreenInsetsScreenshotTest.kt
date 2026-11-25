package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.view.View
import android.view.ViewGroup
import android.widget.ProgressBar
import androidx.annotation.DimenRes
import androidx.fragment.app.Fragment
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.AppBarLayout
import com.google.android.material.color.MaterialColors
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.Robolectric
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import com.google.android.material.R as MaterialR
import dev.proflyder.home.core.designsystem.R as DesignR
import dev.proflyder.home.feature.control.impl.R as ControlR
import dev.proflyder.home.feature.settings.impl.R as SettingsR

@RunWith(ParameterizedRobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = DARK)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ScreenInsetsScreenshotTest(private val screen: InsetScreen) {
    @Test
    fun firstItemSitsOnTheBarEdgeInDark() = checkAtRest("dark")

    @Test
    @Config(qualifiers = LIGHT)
    fun firstItemSitsOnTheBarEdgeInLight() = checkAtRest("light")

    private fun checkAtRest(mode: String) {
        val activity = open()
        val root = activity.window.decorView
        val bar = root.findViewById<View>(R.id.appbar).height
        val spacing = screen.spacing?.let(root.resources::getDimensionPixelSize) ?: 0
        shoot(root, "inset-${screen.name}-$mode-rest.png")
        val content = screenView(activity)
        assertEquals("${screen.name}: first item top", bar + spacing, contentTop(content))
        assertNestedScreensKeepTheirPadding(activity, bar)
        if (screen.listFlushUnderHeader) assertListFlushUnderHeader(content)
        if (screen.scrolledHeader) shootScrolledHeader(activity, mode)
    }

    private fun assertNestedScreensKeepTheirPadding(activity: MainActivity, bar: Int) {
        val host = checkNotNull(activity.supportFragmentManager.findFragmentById(R.id.fragment_container))
        val padded = host.nestedFragments().mapNotNull { it.view }.flatMap { it.descendants() }.filter { it.paddingTop >= bar }
        assertEquals("${screen.name}: nested screens padded by the bar", emptyList<View>(), padded)
    }

    private fun Fragment.nestedFragments(): List<Fragment> =
        childFragmentManager.fragments.flatMap { listOf(it) + it.nestedFragments() }

    private fun View.descendants(): List<View> = when (this) {
        is ViewGroup -> listOf(this) + (0 until childCount).flatMap { getChildAt(it).descendants() }
        else -> listOf(this)
    }

    private fun assertListFlushUnderHeader(content: View) {
        val header = content.innerAppBars().single()
        val list = checkNotNull(content.verticalList())
        assertEquals("${screen.name}: list starts under the inner header", header.windowTop() + header.height, contentTop(list))
    }

    private fun shootScrolledHeader(activity: MainActivity, mode: String) {
        val root = activity.window.decorView
        val list = checkNotNull(screenView(activity).verticalList())
        screenView(activity).innerAppBars().forEach { it.setExpanded(false, false) }
        list.scrollBy(0, (SCROLL_DP * root.resources.displayMetrics.density).toInt())
        settle(activity)
        val shot = shoot(root, "inset-${screen.name}-$mode-scrolled.png")
        val bar = root.findViewById<View>(R.id.appbar).height
        val surface = MaterialColors.getColor(activity, MaterialR.attr.colorSurface, 0)
        val band = (bar until bar + (HEADER_PROBE_DP * root.resources.displayMetrics.density).toInt())
            .flatMap { y -> (0 until shot.width).map { x -> shot.getPixel(x, y) } }
        assertTrue("${screen.name}: pinned header joins the glass bar", band.all { channelDistance(it, surface) <= GLASS_TOLERANCE })
    }

    private fun open(): MainActivity {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        screen.open(activity)
        idle(activity)
        settle(activity)
        screenView(activity).verticalList()?.let { seed(it, activity) }
        settle(activity)
        return activity
    }

    private fun seed(list: RecyclerView, activity: MainActivity) {
        if ((list.adapter?.itemCount ?: 0) > 0) return
        list.layoutManager = LinearLayoutManager(activity)
        list.adapter = TestSectionedRows.titled(activity, screen.headerLayout, screen.headerTitle, DAYS)
        var view: View = list
        while (view !== activity.window.decorView) {
            view.visibility = View.VISIBLE
            view = view.parent as? View ?: break
        }
    }

    private fun screenView(activity: MainActivity): View =
        checkNotNull(activity.supportFragmentManager.findFragmentById(R.id.fragment_container)).requireView()

    private fun contentTop(view: View): Int = when {
        !view.isShown || view.height == 0 || view is ProgressBar -> Int.MAX_VALUE
        view is RecyclerView && view.isVertical() -> view.windowTop() + view.paddingTop
        view.parent is AppBarLayout || view.drawsBackground() -> view.windowTop()
        view is ViewGroup -> (0 until view.childCount).minOfOrNull { contentTop(view.getChildAt(it)) } ?: Int.MAX_VALUE
        else -> view.windowTop()
    }

    private fun View.drawsBackground(): Boolean = this !is AppBarLayout && background?.let { it !is ColorDrawable || it.alpha > 0 } == true

    private fun View.verticalList(): RecyclerView? = when {
        this is RecyclerView && isVertical() -> this
        this is ViewGroup -> (0 until childCount).firstNotNullOfOrNull { getChildAt(it).verticalList() }
        else -> null
    }

    private fun RecyclerView.isVertical(): Boolean = layoutManager?.canScrollVertically() == true

    private fun View.innerAppBars(): List<AppBarLayout> = when (this) {
        is AppBarLayout -> listOf(this).filter { it.id != R.id.appbar }
        is ViewGroup -> (0 until childCount).flatMap { getChildAt(it).innerAppBars() }
        else -> emptyList()
    }

    private fun View.windowTop(): Int = IntArray(2).also(::getLocationInWindow)[1]

    private fun settle(activity: MainActivity) = repeat(SETTLE_PASSES) {
        layout(activity.window.decorView)
        idle(activity)
    }

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun layout(root: View) {
        val density = root.resources.displayMetrics.density
        root.measure(exactly(WIDTH_DP * density), exactly(HEIGHT_DP * density))
        root.layout(0, 0, root.measuredWidth, root.measuredHeight)
    }

    private fun exactly(size: Float) = View.MeasureSpec.makeMeasureSpec(size.toInt(), View.MeasureSpec.EXACTLY)

    private fun shoot(root: View, name: String): Bitmap {
        layout(root)
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        System.getenv("TEST_UNDECLARED_OUTPUTS_DIR")?.let { dir ->
            File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
        }
        return bitmap
    }

    private fun channelDistance(first: Int, second: Int): Int = listOf(
        Color.red(first) - Color.red(second),
        Color.green(first) - Color.green(second),
        Color.blue(first) - Color.blue(second),
    ).maxOf { kotlin.math.abs(it) }

    class InsetScreen(
        val name: String,
        @DimenRes val spacing: Int?,
        val scrolledHeader: Boolean = false,
        val listFlushUnderHeader: Boolean = false,
        val headerLayout: Int = ControlR.layout.item_control_header,
        val headerTitle: Int = ControlR.id.control_header_title,
        val open: (MainActivity) -> Unit,
    ) {
        override fun toString() = name
    }

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun screens(): List<Array<Any>> = (TABS + ROUTES).map { arrayOf(it) }

        private fun tab(id: String, @DimenRes spacing: Int?) = InsetScreen(id, spacing) { it.router.selectTab(id) }

        private fun route(path: String, @DimenRes spacing: Int?) =
            InsetScreen(path.replace('/', '-'), spacing) { it.router.openRoute(path) }

        private val TABS = listOf(
            tab("control", null),
            tab("scenarios", DesignR.dimen.ds_space_m),
            tab("time", DesignR.dimen.ds_space_s),
            tab("chat", DesignR.dimen.ds_space_s),
            tab("system", DesignR.dimen.ds_space_m),
        )

        private val ROUTES = listOf(
            route("access/tokens", DesignR.dimen.ds_space_l),
            route("connection/settings", DesignR.dimen.ds_space_l),
            route("push/settings", DesignR.dimen.ds_space_l),
            route("secrets/catalog", null),
            route("secrets/device", DesignR.dimen.ds_space_xl),
            route("settings/appearance", DesignR.dimen.ds_space_l),
            route("settings/design-system", null),
            route("settings/hub", DesignR.dimen.ds_space_l),
            route("vpn/main", DesignR.dimen.ds_space_l),
            InsetScreen(
                name = "settings-history",
                spacing = null,
                scrolledHeader = true,
                headerLayout = SettingsR.layout.item_history_day,
                headerTitle = SettingsR.id.history_day_title,
                listFlushUnderHeader = true,
            ) { it.router.openRoute("settings/history") },
        )
    }
}

private const val DARK = "w411dp-h891dp-night-xxhdpi"
private const val LIGHT = "w411dp-h891dp-notnight-xxhdpi"
private const val WIDTH_DP = 411
private const val HEIGHT_DP = 891
private const val SCROLL_DP = 260
private const val SETTLE_PASSES = 3
private const val HEADER_PROBE_DP = 4
private const val GLASS_TOLERANCE = 2
private const val PNG_QUALITY = 100
private val DAYS = listOf("Today", "Yesterday", "November 22", "November 21")

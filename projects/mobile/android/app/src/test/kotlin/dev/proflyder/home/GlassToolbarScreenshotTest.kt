package dev.proflyder.home

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.card.MaterialCardView
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import dev.proflyder.home.feature.control.impl.R as ControlR
import kotlinx.coroutines.runBlocking

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class GlassToolbarScreenshotTest {
    @Test
    fun homePinnedHeaderDark() = shootHome(AppTheme.EMERALD, "dark")

    @Test
    @Config(qualifiers = LIGHT)
    fun homePinnedHeaderLight() = shootHome(AppTheme.EMERALD, "light")

    private fun shootHome(theme: AppTheme, mode: String) {
        val activity = launch(theme)
        val root = activity.window.decorView
        val list = root.findViewById<RecyclerView>(ControlR.id.control_list)
        list.layoutManager = LinearLayoutManager(activity)
        list.adapter = SectionedRows(activity)
        applyInsets(activity, 0)
        root.findViewById<View>(ControlR.id.control_error).visibility = View.GONE
        root.findViewById<View>(ControlR.id.control_content).visibility = View.VISIBLE
        list.visibility = View.VISIBLE
        layout(root)
        list.scrollBy(0, dp(root, SCROLL_DP))
        layout(root)
        idle(activity)
        layout(root)

        val bar = root.findViewById<View>(R.id.appbar)
        assertEquals("list starts at the glass bar edge", bar.height, list.paddingTop)
        assertCompactHeader(activity)
        shoot(root, "glass-toolbar-home-$mode.png")
    }

    private fun assertCompactHeader(activity: MainActivity) {
        val header = LayoutInflater.from(activity).inflate(
            ControlR.layout.item_control_header,
            FrameLayout(activity),
            false,
        )
        val density = activity.resources.displayMetrics.density
        val title = header.findViewById<TextView>(ControlR.id.control_header_title)
        assertEquals((14 * density).toInt(), header.paddingTop)
        assertEquals((6 * density).toInt(), header.paddingBottom)
        assertEquals((44 * density).toInt(), header.minimumHeight)
        assertEquals(1, title.maxLines)
    }

    private fun launch(theme: AppTheme): MainActivity {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        runBlocking { app.graph.appearance.selectTheme(theme) }
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        idle(activity)
        return activity
    }

    private fun applyInsets(activity: MainActivity, keyboard: Int) {
        val root = activity.window.decorView
        val builder = WindowInsetsCompat.Builder()
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, dp(root, STATUS_BAR_DP), 0, 0))
            .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, dp(root, NAV_BAR_DP)))
            .setInsets(WindowInsetsCompat.Type.ime(), Insets.of(0, 0, 0, keyboard))
            .setVisible(WindowInsetsCompat.Type.ime(), keyboard > 0)
        layout(root)
        ViewCompat.dispatchApplyWindowInsets(root, builder.build())
        repeat(SETTLE_PASSES) {
            layout(root)
            idle(activity)
        }
        layout(root)
    }

    private fun idle(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(activity.mainLooper).idle()
    }

    private fun layout(root: View) {
        root.measure(exactly(WIDTH_DP, root), exactly(HEIGHT_DP, root))
        root.layout(0, 0, root.measuredWidth, root.measuredHeight)
    }

    private fun shoot(root: View, name: String) {
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private class SectionedRows(context: android.content.Context) : RecyclerView.Adapter<RecyclerView.ViewHolder>(), SectionedAdapter {
        private val inflater = LayoutInflater.from(context)
        private val density = context.resources.displayMetrics.density

        override fun isSectionHeader(position: Int): Boolean = position % (ROWS + 1) == 0

        override fun getItemCount(): Int = SECTIONS * (ROWS + 1)

        override fun getItemViewType(position: Int): Int = if (isSectionHeader(position)) HEADER else ROW

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder = when (viewType) {
            HEADER -> object : RecyclerView.ViewHolder(
                inflater.inflate(ControlR.layout.item_control_header, parent, false),
            ) {}
            else -> object : RecyclerView.ViewHolder(card(parent)) {}
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            when {
                isSectionHeader(position) -> bindHeader(holder.itemView, position / (ROWS + 1))
                else -> (holder.itemView as MaterialCardView).getChildAt(0).let { (it as TextView).text = "Устройство ${position - 1}" }
            }
        }

        private fun bindHeader(view: View, section: Int) {
            view.findViewById<TextView>(ControlR.id.control_header_title).text = TITLES[section]
            view.findViewById<TextView>(ControlR.id.control_header_summary).text = SUMMARIES[section]
        }

        private fun card(parent: ViewGroup): MaterialCardView {
            val margin = (6 * density).toInt()
            val card = MaterialCardView(parent.context).apply {
                layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, (72 * density).toInt()).apply {
                    topMargin = margin
                    bottomMargin = margin
                }
                radius = 16 * density
            }
            card.addView(
                TextView(parent.context).apply {
                    setTextAppearance(com.google.android.material.R.style.TextAppearance_Material3_BodyLarge)
                    setPadding(margin * 3, margin * 3, margin * 3, margin * 3)
                },
                FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT),
            )
            return card
        }
    }

    private companion object {
        const val LIGHT = "w411dp-h891dp-notnight-xxhdpi"
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val STATUS_BAR_DP = 32
        const val NAV_BAR_DP = 24
        const val SCROLL_DP = 90
        const val SECTIONS = 4
        const val ROWS = 6
        const val HEADER = 0
        const val ROW = 1
        const val SETTLE_PASSES = 3
        const val PNG_QUALITY = 100
        val TITLES = listOf("Спальня", "Гостиная", "Кухня", "Прихожая")
        val SUMMARIES = listOf("Включено 2 из 8", "Включено 1 из 5", "Выключено", "Выключено")
    }
}

private fun dp(view: View, value: Int): Int = (value * view.resources.displayMetrics.density).toInt()

private fun exactly(value: Int, view: View): Int =
    View.MeasureSpec.makeMeasureSpec((value * view.resources.displayMetrics.density).toInt(), View.MeasureSpec.EXACTLY)

package dev.proflyder.home.feature.hermes

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Looper
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.R as DesignSystemR
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.designsystem.list.addFullWidthRows
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.feature.hermes.presentation.HermesAdapter
import dev.proflyder.home.feature.hermes.presentation.HermesRow
import dev.proflyder.home.feature.hermes.presentation.HermesRows
import dev.proflyder.home.feature.hermes.presentation.HermesSection
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HermesScreenshotTest {
    private fun themed(): Context {
        val base = ContextThemeWrapper(RuntimeEnvironment.getApplication(), DesignSystemR.style.Base_Theme_Home)
        base.theme.applyStyle(AppTheme.EMERALD.overlayRes, true)
        return base
    }

    private fun render(name: String, rowsOf: (HermesRows) -> List<HermesRow>): RecyclerView {
        val context = themed()
        val list = RecyclerView(context)
        list.layoutManager = StickyHeadersLayoutManager(context)
        val adapter = HermesAdapter(BaseAdapterCallback())
        list.adapter = adapter
        list.addFullWidthRows()
        list.setPadding(0, PADDING, 0, PADDING)
        list.setBackgroundColor(MaterialColors.getColor(list, com.google.android.material.R.attr.colorSurface))
        adapter.submitList(rowsOf(HermesRows(context)))
        shadowOf(Looper.getMainLooper()).idle()
        val width = (WIDTH_DP * context.resources.displayMetrics.density).toInt()
        val height = (HEIGHT_DP * context.resources.displayMetrics.density).toInt()
        list.measure(exactly(width), exactly(height))
        list.layout(0, 0, width, height)
        save(name, list, width, height)
        return list
    }

    private fun save(name: String, view: View, width: Int, height: Int) {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        view.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, QUALITY, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private fun section(section: HermesSection): (HermesRows) -> List<HermesRow> =
        { it.section(section, HermesFixtures.state) }

    private val report: (HermesRows) -> List<HermesRow> = { it.report(HermesFixtures.reports.reports.first()) }

    @Test
    fun overviewLight() = assertEquals(5, render("hermes-overview-light") { it.overview() }.adapter?.itemCount)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun overviewDark() = assertEquals(5, render("hermes-overview-dark") { it.overview() }.adapter?.itemCount)

    @Test
    fun agentsLight() = assertEquals(4, render("hermes-agents-light", section(HermesSection.AGENTS)).adapter?.itemCount)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun agentsDark() = assertEquals(4, render("hermes-agents-dark", section(HermesSection.AGENTS)).adapter?.itemCount)

    @Test
    fun reportsLight() = assertEquals(2, render("hermes-reports-light", section(HermesSection.REPORTS)).adapter?.itemCount)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun reportsDark() = assertEquals(2, render("hermes-reports-dark", section(HermesSection.REPORTS)).adapter?.itemCount)

    @Test
    fun reportLight() = assertEquals(2, render("hermes-report-light", report).adapter?.itemCount)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun reportDark() = assertEquals(2, render("hermes-report-dark", report).adapter?.itemCount)

    @Test
    fun voiceLight() = assertEquals(7, render("hermes-voice-light", section(HermesSection.VOICE)).adapter?.itemCount)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun voiceDark() = assertEquals(7, render("hermes-voice-dark", section(HermesSection.VOICE)).adapter?.itemCount)

    private val toolset: (HermesRows) -> List<HermesRow> = { it.toolset(HermesFixtures.toolsets.named("browser")) }

    @Test
    fun toolsetsLight() = assertEquals(3, render("hermes-toolsets-light", section(HermesSection.TOOLSETS)).adapter?.itemCount)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun toolsetsDark() = assertEquals(3, render("hermes-toolsets-dark", section(HermesSection.TOOLSETS)).adapter?.itemCount)

    @Test
    fun toolsetLight() = assertEquals(6, render("hermes-toolset-light", toolset).adapter?.itemCount)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun toolsetDark() = assertEquals(6, render("hermes-toolset-dark", toolset).adapter?.itemCount)

    @Test
    fun skillsLight() = assertEquals(7, render("hermes-skills-light", section(HermesSection.SKILLS)).adapter?.itemCount)

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun skillsDark() = assertEquals(7, render("hermes-skills-dark", section(HermesSection.SKILLS)).adapter?.itemCount)

    private companion object {
        const val PADDING = 48
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val QUALITY = 100
    }
}

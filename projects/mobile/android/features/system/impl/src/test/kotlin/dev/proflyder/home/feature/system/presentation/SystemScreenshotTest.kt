package dev.proflyder.home.feature.system.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import org.robolectric.RuntimeEnvironment
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.feature.system.SystemFixtures
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SystemScreenshotTest {
    private fun themed(): Context = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
    )

    private fun render(name: String, rowsOf: (SystemRows) -> List<SystemRow>): RecyclerView {
        val context = themed()
        val list = RecyclerView(context)
        list.layoutManager = LinearLayoutManager(context)
        val adapter = SystemAdapter(SystemAdapterCallbacks())
        list.adapter = adapter
        list.setPadding(48, 36, 48, 36)
        list.setBackgroundColor(MaterialColors.getColor(list, com.google.android.material.R.attr.colorSurface))
        adapter.submitList(rowsOf(SystemRows(context)))
        shadowOf(android.os.Looper.getMainLooper()).idle()
        val width = (411 * context.resources.displayMetrics.density).toInt()
        val height = (891 * context.resources.displayMetrics.density).toInt()
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
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private fun overview(snapshot: SystemSnapshot): (SystemRows) -> List<SystemRow> = { it.overview(snapshot) }

    @Test
    fun overviewWithProblems() {
        val list = render("system-overview", overview(SystemFixtures.snapshot))
        assertEquals(7, list.adapter?.itemCount)
    }

    @Test
    fun overviewWithoutSources() {
        val list = render("system-overview-no-data", overview(SystemFixtures.empty))
        assertEquals(5, list.adapter?.itemCount)
    }

    @Test
    fun clusterDetail() {
        val list = render("system-cluster") { it.detail(SystemSection.CLUSTER, SystemFixtures.snapshot) }
        assertEquals(5, list.adapter?.itemCount)
    }

    @Test
    @Config(qualifiers = "ru-night-w411dp-h891dp-xxhdpi")
    fun guestsDetailDark() {
        val list = render("system-guests-dark") { it.detail(SystemSection.GUESTS, SystemFixtures.snapshot) }
        assertEquals(4, list.adapter?.itemCount)
    }
}

package dev.proflyder.home.feature.control.presentation

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Looper
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.ConcatAdapter
import dev.proflyder.home.core.designsystem.glass.GlassBlockHost
import dev.proflyder.home.core.designsystem.glass.GlassChromeLayout
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.ui.applyContentInsets
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
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
class HomeScrolledGlassTest {
    @Test
    fun alarmSlotScrollsUnderTheBarAndThePinnedRoomJoinsTheBarGlass() {
        val home = open()
        val manager = home.binding.controlList.layoutManager as StickyHeadersLayoutManager
        assertEquals("alarm slot is the first list item", BAR.toFloat(), home.binding.controlHeader.translationY)
        assertEquals(BAR, home.binding.controlList.paddingTop)
        assertNull(manager.stickyHeaderView)
        assertEquals("nothing pinned: the glass is the bar", BAR, home.chrome.glassBottom)
        shoot(home, "home-glass-rest.png")

        home.scroll(home.binding.controlHeader.height + ROOM_SCROLL)
        val pinned = checkNotNull(manager.stickyHeaderView) { "a room header is pinned" }
        val slot = home.binding.controlHeader
        assertTrue("alarm slot went under the bar", slot.translationY + slot.height <= BAR)
        assertEquals("pinned room sits on the bar edge", BAR, pinned.top)
        assertEquals("pinned room has no own surface", 0f, pinned.alpha)
        assertEquals("bar and pinned room are one glass rect", BAR + pinned.height, home.chrome.glassBottom)
        shoot(home, "home-glass-pinned.png")
    }

    private fun open(): Home {
        val controller = Robolectric.buildActivity(Activity::class.java)
        controller.get().setTheme(DesignR.style.Base_Theme_Home)
        val activity = controller.setup().get()
        val themed = ContextThemeWrapper(activity, DesignR.style.Base_Theme_Home).apply { theme.applyStyle(AppTheme.EMERALD.overlayRes, true) }
        val chrome = GlassChromeLayout(themed)
        val context = HostContext(themed, chrome)
        val binding = FragmentControlBinding.inflate(LayoutInflater.from(context).cloneInContext(context))
        ControlListRenderer(binding) { }
        binding.controlAlarmsSlot.layoutParams.height = (SLOT_DP * activity.resources.displayMetrics.density).toInt()
        binding.controlProgress.visibility = View.GONE
        binding.controlContent.visibility = View.VISIBLE
        val adapter = (binding.controlList.adapter as ConcatAdapter).adapters.filterIsInstance<ControlAdapter>().single()
        adapter.submitList(rooms())
        binding.controlList.applyContentInsets()
        activity.setContentView(
            FrameLayout(activity).apply {
                clipChildren = false
                addView(binding.root)
                addView(chrome, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, BAR))
            },
        )
        chrome.bindBackdrop(binding.root)
        val home = Home(activity, binding, chrome)
        home.settle()
        ViewCompat.dispatchApplyWindowInsets(binding.root, insets())
        home.settle()
        return home
    }

    private fun insets(): WindowInsetsCompat = WindowInsetsCompat.Builder()
        .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, BAR, 0, 0))
        .build()

    private fun shoot(home: Home, name: String) {
        val root = home.activity.window.decorView
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun rooms(): List<ControlItem> = ROOMS.flatMapIndexed { room, title ->
        listOf<ControlItem>(ControlItem.Header("room:$title", title, summary = SUMMARIES[room])) +
            DEVICES.mapIndexed { index, name -> ControlItem.Device(device("$room-$index", name, title), busy = false, statesKnown = true) }
    }

    private fun device(id: String, title: String, room: String) = ControlEntry(
        id = "light.$id",
        kind = ControlKind.LIGHT,
        title = title,
        room = room,
        icon = "lightbulb-outline",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.ON,
        available = true,
    )

    private class Home(val activity: Activity, val binding: FragmentControlBinding, val chrome: GlassChromeLayout) {
        fun scroll(dy: Int) {
            binding.controlList.scrollBy(0, dy)
            settle()
        }

        fun settle() = repeat(PASSES) { settleOnce() }

        private fun settleOnce() {
            activity.window.decorView.requestLayout()
            shadowOf(Looper.getMainLooper()).idle()
        }
    }

    private class HostContext(base: Context, private val chrome: GlassChromeLayout) : ContextWrapper(base), GlassBlockHost by chrome

    private companion object {
        const val BAR = 300
        const val SLOT_DP = 72
        const val ROOM_SCROLL = 400
        const val PASSES = 3
        const val PNG_QUALITY = 100
        val ROOMS = listOf("Спальня", "Гостиная", "Кухня")
        val SUMMARIES = listOf("Включено 5 из 8", "Включено 1 из 4", "Выключено")
        val DEVICES = listOf("Лампа", "Светильник", "Лента", "Торшер", "Ночник")
    }
}

package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.content.ContextWrapper
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Bundle
import android.os.Looper
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.appcompat.view.ContextThemeWrapper
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.fragment.app.FragmentActivity
import androidx.recyclerview.widget.ConcatAdapter
import dev.proflyder.home.core.designsystem.glass.GlassChromeLayout
import dev.proflyder.home.core.designsystem.glass.GlassEdgeOwner
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.presentation.SectionFragment
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.ui.applyContentInsets
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsUpcomingBinding
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmsUpcomingBinding
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import dev.proflyder.home.core.designsystem.R as DesignR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HomeAlarmGapsTest {
    @Test
    fun oneAlarmSitsOnTheCardGridWithoutEmptySlots() = check("home-one-alarm-dark.png")

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-notnight-xxhdpi")
    fun oneAlarmSitsOnTheCardGridWithoutEmptySlotsLight() = check("home-one-alarm-light.png")

    private fun check(name: String) {
        val home = open()
        val card = home.window(home.alarm)
        val chips = home.window(home.chips)
        val step = home.activity.resources.getDimensionPixelSize(R.dimen.control_card_gap)
        assertEquals("the section takes no window insets", step, home.binding.controlAlarmsSlot.getChildAt(0).paddingTop)
        val edge = home.activity.resources.getDimensionPixelSize(R.dimen.control_card_edge)
        shoot(home, name)
        assertEquals("gap from the bar to the alarm", step, card.top - BAR)
        assertEquals("gap from the alarm to the filters", step, chips.top - card.bottom)
        assertEquals("alarm card edge", edge, card.left)
        assertEquals("device card edge", edge, home.window(home.device).left)
        assertEquals("same row height as a device card", home.window(home.device).height(), card.height())
    }

    private fun open(): Home {
        val controller = Robolectric.buildActivity(FragmentActivity::class.java)
        controller.get().setTheme(DesignR.style.Base_Theme_Home)
        val activity = controller.setup().get()
        val themed = ContextThemeWrapper(activity, DesignR.style.Base_Theme_Home).apply { theme.applyStyle(AppTheme.EMERALD.overlayRes, true) }
        val chrome = GlassChromeLayout(themed)
        val context = HostContext(themed, chrome)
        val binding = FragmentControlBinding.inflate(LayoutInflater.from(context).cloneInContext(context))
        ControlListRenderer(binding) { }
        binding.controlProgress.visibility = View.GONE
        binding.controlContent.visibility = View.VISIBLE
        val adapter = (binding.controlList.adapter as ConcatAdapter).adapters.filterIsInstance<ControlAdapter>().single()
        adapter.submitList(items())
        binding.controlList.applyContentInsets()
        activity.setContentView(
            FrameLayout(activity).apply {
                clipChildren = false
                addView(binding.root)
                addView(chrome, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, BAR))
            },
        )
        chrome.bindBackdrop(binding.root)
        activity.supportFragmentManager.beginTransaction().add(binding.controlAlarmsSlot.id, OneAlarm()).commitNow()
        val home = Home(activity, binding)
        home.settle()
        ViewCompat.dispatchApplyWindowInsets(binding.root, WindowInsetsCompat.Builder().setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, BAR, 0, 0)).build())
        home.settle()
        return home
    }

    private fun items(): List<ControlItem> = listOf(
        ControlItem.Filters(listOf(ControlItem.FilterChip(ControlFilter.All, "Все · 2", 2), ControlItem.FilterChip(ControlFilter.Lights, "Свет · 2", 2)), ControlFilter.All),
        ControlItem.Header("room:Спальня", "Спальня", summary = "Включено 2 из 2"),
    ) + DEVICES.mapIndexed { index, title -> ControlItem.Device(device("d$index", title), busy = false, statesKnown = true) }

    private fun device(id: String, title: String) = ControlEntry(
        id = "light.$id",
        kind = ControlKind.LIGHT,
        title = title,
        room = "Спальня",
        icon = "lightbulb-outline",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.ON,
        available = true,
    )

    private fun shoot(home: Home, name: String) {
        val root = home.activity.window.decorView
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    class OneAlarm : SectionFragment<FragmentAlarmsUpcomingBinding, Unit, Unit, Unit>(FragmentAlarmsUpcomingBinding::inflate) {
        override val viewModel: StateViewModel<Unit, Unit, Unit> = Still()

        override fun onViewReady(binding: FragmentAlarmsUpcomingBinding, savedInstanceState: Bundle?) {
            binding.root.visibility = View.VISIBLE
            val first = ItemAlarmsUpcomingBinding.bind(binding.alarmsUpcomingFirst)
            first.alarmsUpcomingTitle.append(TITLE)
            first.alarmsUpcomingDetail.append(DETAIL)
            binding.alarmsUpcomingSecond.visibility = View.GONE
            binding.alarmsUpcomingThird.visibility = View.GONE
        }
    }

    private class Still : StateViewModel<Unit, Unit, Unit>(Unit) {
        override fun handle(intent: Unit) = Unit
    }

    private class Home(val activity: FragmentActivity, val binding: FragmentControlBinding) {
        val alarm: View get() = binding.controlAlarmsSlot.findViewById(dev.proflyder.home.feature.alarms.impl.R.id.alarms_upcoming_first)
        val chips: View get() = binding.controlList.findViewById(R.id.control_filters)
        val device: View get() = (0 until binding.controlList.childCount).map(binding.controlList::getChildAt)
            .first { it.findViewById<TextView?>(R.id.control_device_title) != null }

        fun window(view: View): android.graphics.Rect {
            val at = IntArray(2).also(view::getLocationInWindow)
            return android.graphics.Rect(at[0], at[1], at[0] + view.width, at[1] + view.height)
        }

        fun settle() = repeat(PASSES) {
            activity.window.decorView.requestLayout()
            shadowOf(Looper.getMainLooper()).idle()
        }
    }

    private class HostContext(base: Context, private val chrome: GlassChromeLayout) : ContextWrapper(base), GlassEdgeOwner by chrome

    private companion object {
        const val BAR = 300
        const val PASSES = 4
        const val PNG_QUALITY = 100
        const val TITLE = "завтра 10:00 · Работа"
        const val DETAIL = "через 16 ч 40 мин"
        val DEVICES = listOf("Лампа", "Светильник")
    }
}

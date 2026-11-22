package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import com.google.android.material.materialswitch.MaterialSwitch
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.ColorRole
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.R
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class DeviceStateScreenshotTest {
    private val callback = ControlAdapterCallback {}

    @Test
    @Config(qualifiers = "notnight")
    fun currentLight() = check(AppTheme.CURRENT, "light")

    @Test
    @Config(qualifiers = "night")
    fun currentDark() = check(AppTheme.CURRENT, "dark")

    @Test
    @Config(qualifiers = "notnight")
    fun neonLight() = check(AppTheme.NEON, "light")

    @Test
    @Config(qualifiers = "night")
    fun neonDark() = check(AppTheme.NEON, "dark")

    @Test
    @Config(qualifiers = "notnight")
    fun systemLight() = check(AppTheme.SYSTEM, "light")

    @Test
    @Config(qualifiers = "night")
    fun systemDark() = check(AppTheme.SYSTEM, "dark")

    private fun check(theme: AppTheme, mode: String) {
        val context = themed(theme)
        val column = render(context)
        val lampOn = column.getChildAt(0)
        val lampOff = column.getChildAt(1)
        assertStatesDiffer(lampOn, lampOff, R.id.control_device_icon, R.id.control_device_state)
        val scheme = scheme(theme, mode)
        assertEquals(scheme.stateOn, fill(lampOn.findViewById(R.id.control_device_icon)))
        assertEquals(scheme.stateOnText, lampOn.findViewById<TextView>(R.id.control_device_state).currentTextColor)
        assertEquals(scheme.onSurfaceVariant, lampOff.findViewById<TextView>(R.id.control_device_state).currentTextColor)
        assertTilesDiffer(column.getChildAt(3))
        assertSwitchMatchesTile(context, fill(lampOn.findViewById(R.id.control_device_icon)))
        assertAligned(lampOn, column.getChildAt(2))
        assertSameColumn(lampOn, column.getChildAt(2))
        save(column, "device-states-${theme.key}-$mode.png")
    }

    private fun scheme(theme: AppTheme, mode: String) = when (mode) {
        "dark" -> theme.palette.dark
        else -> theme.palette.light
    }

    private fun themed(theme: AppTheme): Context {
        val base = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        return ContextThemeWrapper(base, theme.overlayRes)
    }

    private fun render(context: Context): LinearLayout {
        val column = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        column.setBackgroundColor(MaterialColors.getColor(context, ColorRole.SURFACE.attr, 0))
        val adapter = ControlAdapter(callback)
        val items = items()
        adapter.submitList(items)
        items.indices.forEach { column.addView(holderView(adapter, column, it)) }
        val metrics = context.resources.displayMetrics
        val width = (WIDTH_DP * metrics.density).toInt()
        column.measure(exactly(width), View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED))
        column.layout(0, 0, width, column.measuredHeight)
        return column
    }

    private fun holderView(adapter: ControlAdapter, parent: LinearLayout, index: Int): View {
        val holder = adapter.onCreateViewHolder(FrameLayout(parent.context), adapter.getItemViewType(index))
        adapter.onBindViewHolder(holder, index)
        return holder.itemView
    }

    private fun items(): List<ControlItem> = listOf(
        ControlItem.Device(entry("light.lamp", "Лампа", PowerState.ON), busy = false, statesKnown = true),
        ControlItem.Device(entry("light.floor", "Торшер", PowerState.OFF), busy = false, statesKnown = true),
        ControlItem.DeviceGroup(
            deviceId = "lamp2",
            title = "Светильник",
            primary = listOf(ControlItem.DeviceGroup.Row(entry("light.sconce", "Светильник", PowerState.ON), false)),
            settings = listOf(ControlItem.DeviceGroup.Row(entry("switch.mode", "Режим", PowerState.OFF), false)),
            expanded = false,
            statesKnown = true,
            mainId = "light.sconce",
        ),
        station(),
    )

    private fun station() = ControlItem.DeviceGroup(
        deviceId = "station",
        title = "Станция",
        primary = listOf(
            ControlItem.DeviceGroup.Row(entry("switch.mics", "Микрофоны", PowerState.ON), false),
            ControlItem.DeviceGroup.Row(entry("switch.ring", "Подсветка", PowerState.OFF), false),
            ControlItem.DeviceGroup.Row(entry("switch.clock", "Часы на дисплее", PowerState.ON), false),
        ),
        settings = emptyList(),
        expanded = false,
        statesKnown = true,
    )

    private fun entry(id: String, title: String, power: PowerState) = ControlEntry(
        id = id,
        kind = ControlKind.SWITCH,
        title = title,
        room = "",
        icon = "power",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = power,
        available = true,
    )

    private fun assertStatesDiffer(on: View, off: View, iconId: Int, statusId: Int) {
        assertNotEquals(fill(on.findViewById(iconId)), fill(off.findViewById(iconId)))
        assertNotEquals(
            on.findViewById<TextView>(statusId).currentTextColor,
            off.findViewById<TextView>(statusId).currentTextColor,
        )
    }

    private fun assertTilesDiffer(card: View) {
        val icons = mutableListOf<ImageView>()
        collect(card, R.id.control_tile_icon, icons)
        icons.retainAll { it.rootTile().visibility == View.VISIBLE }
        assertEquals(TILE_COUNT, icons.size)
        assertNotEquals(fill(icons[0]), fill(icons[1]))
        assertEquals(fill(icons[0]), fill(icons[2]))
        assertStatesDiffer(icons[0].rootTile(), icons[1].rootTile(), R.id.control_tile_icon, R.id.control_tile_label)
    }

    private fun ImageView.rootTile(): View = parent.parent as View

    private fun collect(view: View, id: Int, into: MutableList<ImageView>) {
        if (view.id == id) into += view as ImageView
        val group = view as? android.view.ViewGroup ?: return
        (0 until group.childCount).forEach { collect(group.getChildAt(it), id, into) }
    }

    private fun assertSwitchMatchesTile(context: Context, tileFill: Int) {
        val switch = MaterialSwitch(context)
        switch.isChecked = true
        val track = switch.trackTintList ?: error("no track tint")
        val checked = intArrayOf(android.R.attr.state_enabled, android.R.attr.state_checked)
        assertEquals(tileFill, track.getColorForState(checked, 0))
        val offTrack = track.getColorForState(intArrayOf(android.R.attr.state_enabled), 0)
        assertEquals(MaterialColors.getColor(context, ColorRole.SURFACE_CONTAINER_HIGHEST.attr, 0), offTrack)
    }

    private fun assertAligned(device: View, group: View) {
        val deviceIcon = device.findViewById<View>(R.id.control_device_icon)
        val groupIcon = group.findViewById<View>(R.id.control_row_icon)
        val deviceCenter = top(deviceIcon, device) + deviceIcon.height / 2
        val groupCenter = top(groupIcon, group) + groupIcon.height / 2
        assertEquals(deviceCenter.toFloat(), groupCenter.toFloat(), device.resources.displayMetrics.density)
    }

    private fun assertSameColumn(device: View, group: View) {
        val deviceIcon = device.findViewById<View>(R.id.control_device_icon)
        val groupIcon = group.findViewById<View>(R.id.control_row_icon)
        val deviceTitle = device.findViewById<View>(R.id.control_device_title)
        val groupTitle = group.findViewById<View>(R.id.control_row_title)
        assertEquals("icon left", left(deviceIcon, device), left(groupIcon, group))
        assertEquals("icon width", deviceIcon.width, groupIcon.width)
        assertEquals("icon height", deviceIcon.height, groupIcon.height)
        assertEquals("title left", left(deviceTitle, device), left(groupTitle, group))
    }

    private fun left(view: View, root: View): Int = when (view) {
        root -> 0
        else -> view.left + left(view.parent as View, root)
    }

    private fun top(view: View, root: View): Int = when (view) {
        root -> 0
        else -> view.top + top(view.parent as View, root)
    }

    private fun fill(icon: ImageView): Int = icon.backgroundTintList?.defaultColor ?: 0

    private fun save(view: View, name: String) {
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        view.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val WIDTH_DP = 411
        const val PNG_QUALITY = 100
        const val TILE_COUNT = 3
    }
}

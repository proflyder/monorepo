package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.view.LayoutInflater
import android.view.View
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.widget.NestedScrollView
import com.google.android.material.bottomsheet.BottomSheetDialog
import dev.proflyder.home.core.model.CatalogGrouping
import dev.proflyder.home.core.model.CatalogGroupingLabels
import dev.proflyder.home.core.model.CatalogDeviceGroup
import dev.proflyder.home.core.model.CatalogRoomGroup
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemCatalogRowBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemCatalogSettingsBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.SheetChoiceBinding

/**
 * Catalog picker grouped by room and then by device.
 *
 * Group titles never contain raw entity ids: a missing device falls back to a
 * meaningful entry title or a localized "Scenarios HA"/"Other" label. Secondary
 * service entities (`role = SETTING`) are hidden behind a switch at the bottom.
 */
internal class CatalogPickerDialog(
    private val context: Context,
    private val entries: List<ControlEntry>,
    private val onSelect: (ControlEntry) -> Unit,
) {
    private val dialog = BottomSheetDialog(context)
    private val views = SheetChoiceBinding.inflate(LayoutInflater.from(context))
    private val labels = ResourceGroupingLabels(context)
    private var includeSettings = false

    init {
        views.choiceTitle.setText(R.string.scenarios_catalog_title)
        renderList()
        dialog.setContentView(NestedScrollView(context).apply { addView(views.root) })
    }

    fun show() = dialog.show()

    private fun renderList() {
        val list = views.choiceItems
        list.removeAllViews()
        val rooms = CatalogGrouping.rooms(entries, includeSettings, labels)
        if (rooms.isEmpty()) {
            list += header(context.getString(R.string.scenarios_catalog_empty))
        }
        rooms.forEach { room -> addRoom(list, room) }
        val settingsCount = CatalogGrouping.settingsCount(entries)
        if (settingsCount > 0) {
            list += settingsToggle(settingsCount)
        }
    }

    private fun addRoom(list: android.widget.LinearLayout, room: CatalogRoomGroup) {
        list += header(room.room.ifBlank { context.getString(R.string.scenarios_catalog_other_room) })
        room.devices.forEach { group -> addDevice(list, group) }
    }

    private fun addDevice(list: android.widget.LinearLayout, group: CatalogDeviceGroup) {
        if (CatalogGrouping.needsDeviceHeader(group, labels)) {
            list += subheader(group.title)
        }
        group.entries.forEach { entry -> list += catalogRow(entry) }
    }

    private fun catalogRow(entry: ControlEntry): View {
        val row = ItemCatalogRowBinding.inflate(LayoutInflater.from(context))
        row.catalogRowIcon.setImageResource(iconOf(entry))
        row.catalogRowTitle.text = CatalogGrouping.entryLabel(entry, labels)
        val actions = actionSummary(entry)
        row.catalogRowSubtitle.text = actions
        row.catalogRowSubtitle.setVisible(actions.isNotBlank())
        row.root.setOnClickListener {
            onSelect(entry)
            dialog.dismiss()
        }
        return row.root
    }

    private fun settingsToggle(count: Int): View {
        val views = ItemCatalogSettingsBinding.inflate(LayoutInflater.from(context))
        views.catalogShowSettings.text =
            context.getString(R.string.scenarios_catalog_show_settings, count)
        views.catalogShowSettings.isChecked = includeSettings
        views.catalogShowSettings.setOnCheckedChangeListener { _, checked ->
            includeSettings = checked
            renderList()
        }
        return views.root
    }

    private fun actionSummary(entry: ControlEntry): String {
        if (entry.actions.isEmpty()) return ""
        if (entry.actions.contains(ControlAction.SPEAK)) {
            return context.getString(R.string.scenarios_actions_speak)
        }
        val parts = mutableListOf<String>()
        if (entry.actions.contains(ControlAction.RUN)) {
            parts += context.getString(R.string.scenarios_actions_run)
        }
        if (entry.actions.contains(ControlAction.TURN_ON) || entry.actions.contains(ControlAction.TURN_OFF)) {
            parts += context.getString(R.string.scenarios_actions_on_off)
        }
        if (entry.actions.contains(ControlAction.SET)) {
            parts += context.getString(R.string.scenarios_actions_adjust)
        }
        return parts.distinct().joinToString(SEPARATOR)
    }

    private fun iconOf(entry: ControlEntry): Int = when (entry.kind) {
        ControlKind.LIGHT -> R.drawable.scenarios_ic_light
        ControlKind.SWITCH -> R.drawable.scenarios_ic_power
        ControlKind.SCENARIO -> R.drawable.scenarios_ic_play
        ControlKind.ACTION -> R.drawable.scenarios_ic_speak
        ControlKind.PROFILE -> R.drawable.scenarios_ic_profile
        ControlKind.OTHER -> R.drawable.scenarios_ic_devices
    }

    private fun header(text: String): TextView = TextView(context).apply {
        this.text = text
        setTextAppearance(com.google.android.material.R.style.TextAppearance_Material3_TitleMedium)
        setPadding(dp(24), dp(16), dp(24), dp(4))
    }

    private fun subheader(text: String): TextView = TextView(context).apply {
        this.text = text
        setTextAppearance(com.google.android.material.R.style.TextAppearance_Material3_LabelLarge)
        setPadding(dp(24), dp(8), dp(24), dp(2))
    }

    private operator fun LinearLayout.plusAssign(view: View) {
        addView(view)
    }

    private fun dp(value: Int): Int = (value * context.resources.displayMetrics.density).toInt()

    private class ResourceGroupingLabels(private val context: Context) : CatalogGroupingLabels {
        override fun scenariosHa(): String = context.getString(R.string.scenarios_catalog_group_scenarios)
        override fun other(): String = context.getString(R.string.scenarios_catalog_group_other)
        override fun light(): String = context.getString(R.string.scenarios_catalog_kind_light)
        override fun switch(): String = context.getString(R.string.scenarios_catalog_kind_switch)
        override fun scenario(): String = context.getString(R.string.scenarios_catalog_kind_scenario)
        override fun action(): String = context.getString(R.string.scenarios_catalog_kind_action)
        override fun device(): String = context.getString(R.string.scenarios_catalog_kind_device)
    }

    private companion object {
        const val SEPARATOR = " · "
    }
}

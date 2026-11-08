package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.view.LayoutInflater
import android.view.View
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.widget.NestedScrollView
import com.google.android.material.bottomsheet.BottomSheetDialog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemCatalogRowBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.SheetChoiceBinding

/**
 * Catalog picker grouped by room and then by device. Rows are ordinary 56dp
 * list rows with the device name as the main label; the technical entity title
 * is only used when it carries real information (see
 * [ControlEntry.displayName]).
 */
internal class CatalogPickerDialog(
    private val context: Context,
    entries: List<ControlEntry>,
    private val onSelect: (ControlEntry) -> Unit,
) {
    private val dialog = BottomSheetDialog(context)

    init {
        val views = SheetChoiceBinding.inflate(LayoutInflater.from(context))
        views.choiceTitle.setText(R.string.scenarios_catalog_title)
        val list = views.choiceItems
        if (entries.isEmpty()) {
            list += header(context.getString(R.string.scenarios_catalog_empty))
        }
        entries.groupBy { it.room }.forEach { (room, roomEntries) ->
            list += header(room.ifBlank { context.getString(R.string.scenarios_catalog_other_room) })
            roomEntries.groupBy { it.device.ifBlank { it.id } }.forEach { (device, deviceEntries) ->
                list += subheader(device)
                deviceEntries.forEach { entry -> list += catalogRow(entry) }
            }
        }
        dialog.setContentView(NestedScrollView(context).apply { addView(views.root) })
    }

    fun show() = dialog.show()

    private fun catalogRow(entry: ControlEntry): View {
        val row = ItemCatalogRowBinding.inflate(LayoutInflater.from(context))
        row.catalogRowIcon.setImageResource(R.drawable.scenarios_ic_devices)
        row.catalogRowTitle.text = entry.displayName
        val actions = entry.actions.joinToString(", ") { ScenarioFormat.actionLabel(context, it.wire) }
        val subtitle = listOfNotNull(entry.secondaryName, actions.ifBlank { null }).joinToString(SEPARATOR)
        row.catalogRowSubtitle.text = subtitle
        row.catalogRowSubtitle.setVisible(subtitle.isNotBlank())
        row.root.setOnClickListener {
            onSelect(entry)
            dialog.dismiss()
        }
        return row.root
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

    private companion object {
        const val SEPARATOR = " · "
    }
}

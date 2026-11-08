package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.widget.NestedScrollView
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.model.ControlEntry

/**
 * Catalog picker grouped by room and then by device. The list is whatever the
 * home control service reports; nothing is hard-coded.
 */
internal class CatalogPickerDialog(
    private val context: Context,
    entries: List<ControlEntry>,
    private val onSelect: (ControlEntry) -> Unit,
) {
    private val dialog = BottomSheetDialog(context)

    init {
        val list = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(context, 20), dp(context, 12), dp(context, 20), dp(context, 24))
        }
        if (entries.isEmpty()) {
            list += header(context, context.getString(dev.proflyder.home.feature.scenarios.impl.R.string.scenarios_catalog_empty))
        }
        entries.groupBy { it.room }.forEach { (room, roomEntries) ->
            list += header(
                context,
                room.ifBlank { context.getString(dev.proflyder.home.feature.scenarios.impl.R.string.scenarios_catalog_other_room) },
            )
            roomEntries.groupBy { it.device.ifBlank { it.id } }.forEach { (device, deviceEntries) ->
                list += subheader(context, device)
                deviceEntries.forEach { entry ->
                    list += MaterialButton(context, null, com.google.android.material.R.style.Widget_Material3_Button_OutlinedButton).apply {
                        text = entryLabel(entry)
                        isAllCaps = false
                        gravity = android.view.Gravity.START or android.view.Gravity.CENTER_VERTICAL
                        layoutParams = LinearLayout.LayoutParams(
                            ViewGroup.LayoutParams.MATCH_PARENT,
                            ViewGroup.LayoutParams.WRAP_CONTENT,
                        )
                        setOnClickListener {
                            onSelect(entry)
                            dialog.dismiss()
                        }
                    }
                }
            }
        }
        val scroll = NestedScrollView(context).apply { addView(list) }
        dialog.setContentView(scroll)
        dialog.setTitle(context.getString(dev.proflyder.home.feature.scenarios.impl.R.string.scenarios_catalog_title))
    }

    fun show() = dialog.show()

    private fun entryLabel(entry: ControlEntry): String {
        val actions = entry.actions.joinToString(", ") {
            ScenarioFormat.actionLabel(context, it.wire)
        }
        return if (actions.isEmpty()) entry.title else "${entry.title} · $actions"
    }

    private fun header(context: Context, text: String): TextView = TextView(context).apply {
        this.text = text
        setTextAppearance(com.google.android.material.R.style.TextAppearance_Material3_TitleMedium)
        setPadding(0, dp(context, 16), 0, dp(context, 4))
    }

    private fun subheader(context: Context, text: String): TextView = TextView(context).apply {
        this.text = text
        setTextAppearance(com.google.android.material.R.style.TextAppearance_Material3_LabelLarge)
        setPadding(0, dp(context, 8), 0, dp(context, 2))
    }

    private operator fun LinearLayout.plusAssign(view: android.view.View) {
        addView(view)
    }

    private fun dp(context: Context, value: Int): Int =
        (value * context.resources.displayMetrics.density).toInt()
}

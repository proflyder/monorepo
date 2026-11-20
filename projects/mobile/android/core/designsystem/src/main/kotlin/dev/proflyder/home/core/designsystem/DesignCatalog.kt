package dev.proflyder.home.core.designsystem

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.core.view.doOnLayout
import com.google.android.material.button.MaterialButton
import com.google.android.material.textfield.TextInputLayout

object DesignCatalog {
    val sections: List<Pair<String, Int>> = listOf(
        "buttons" to R.id.ds_catalog_buttons,
        "icon-buttons" to R.id.ds_catalog_icon_buttons,
        "fabs" to R.id.ds_catalog_fabs,
        "fields" to R.id.ds_catalog_fields,
        "cards" to R.id.ds_catalog_cards,
        "chips" to R.id.ds_catalog_chips,
        "lists" to R.id.ds_catalog_lists,
        "states" to R.id.ds_catalog_states,
    )

    internal val loadingButtons = listOf(
        R.id.ds_catalog_loading_filled_s,
        R.id.ds_catalog_loading_filled_m,
        R.id.ds_catalog_loading_filled_l,
        R.id.ds_catalog_loading_tonal_s,
        R.id.ds_catalog_loading_tonal_m,
        R.id.ds_catalog_loading_tonal_l,
        R.id.ds_catalog_loading_outlined_s,
        R.id.ds_catalog_loading_outlined_m,
        R.id.ds_catalog_loading_outlined_l,
        R.id.ds_catalog_loading_text_s,
        R.id.ds_catalog_loading_text_m,
        R.id.ds_catalog_loading_text_l,
        R.id.ds_catalog_loading_destructive_s,
        R.id.ds_catalog_loading_destructive_m,
        R.id.ds_catalog_loading_destructive_l,
    )

    fun inflate(inflater: LayoutInflater, parent: ViewGroup?): View {
        val root = inflater.inflate(R.layout.ds_catalog, parent, false)
        bind(root)
        return root
    }

    fun bind(root: View) {
        val resources = root.resources
        loadingButtons.map { root.findViewById<MaterialButton>(it) }.forEach { button ->
            button.doOnLayout { button.setLoading(true) }
        }
        root.findViewById<TextInputLayout>(R.id.ds_catalog_field_error).error = resources.getString(R.string.ds_catalog_field_error)
        root.findViewById<ListRow>(R.id.ds_catalog_row_switch).switch.isChecked = true
        val pills = root.findViewById<PillSegments>(R.id.ds_catalog_pills)
        pills.setLabels(listOf(R.string.ds_catalog_chip_filter, R.string.ds_catalog_chip_assist).map(resources::getText))
        pills.select(0)
    }
}

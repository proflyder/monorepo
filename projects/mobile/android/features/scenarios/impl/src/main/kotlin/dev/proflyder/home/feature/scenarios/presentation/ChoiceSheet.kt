package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.view.LayoutInflater
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import com.google.android.material.bottomsheet.BottomSheetDialog
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemChoiceRowBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.SheetChoiceBinding

/**
 * Material bottom sheet with one full-width, 56dp action row per choice:
 * 24dp icon, body-large label, ripple over the whole row, no filled button.
 */
internal class ChoiceSheet(
    private val context: Context,
    @StringRes titleRes: Int,
    private val choices: List<Choice>,
) {
    data class Choice(
        val label: CharSequence,
        @DrawableRes val iconRes: Int,
        val onClick: () -> Unit,
    )

    private val dialog = BottomSheetDialog(context)

    init {
        val views = SheetChoiceBinding.inflate(LayoutInflater.from(context))
        views.choiceTitle.setText(titleRes)
        choices.forEach { choice -> views.choiceItems.addView(choiceRow(choice, views.choiceItems)) }
        dialog.setContentView(views.root)
    }

    private fun choiceRow(choice: Choice, parent: android.view.ViewGroup): android.view.View {
        val row = ItemChoiceRowBinding.inflate(LayoutInflater.from(context), parent, false)
        row.choiceIcon.setImageResource(choice.iconRes)
        row.choiceLabel.text = choice.label
        row.root.setOnClickListener {
            dialog.dismiss()
            choice.onClick()
        }
        return row.root
    }

    fun show() = dialog.show()
}

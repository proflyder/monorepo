package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.view.Gravity
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.annotation.StringRes
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.button.MaterialButton

/**
 * Small bottom sheet with one full-width text action per row. Used instead of
 * a row of narrow buttons so labels never break mid-word on a phone.
 */
internal class ChoiceSheet(
    context: Context,
    @StringRes titleRes: Int,
    private val choices: List<Choice>,
) {
    data class Choice(@StringRes val labelRes: Int, val onClick: () -> Unit)

    private val dialog = BottomSheetDialog(context)

    init {
        val root = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(context, 20), dp(context, 16), dp(context, 20), dp(context, 24))
        }
        root.addView(
            TextView(context).apply {
                setText(titleRes)
                setTextAppearance(com.google.android.material.R.style.TextAppearance_Material3_TitleMedium)
                setPadding(0, 0, 0, dp(context, 8))
            },
        )
        choices.forEach { choice ->
            root.addView(
                MaterialButton(
                    context,
                    null,
                    com.google.android.material.R.style.Widget_Material3_Button_TextButton,
                ).apply {
                    setText(choice.labelRes)
                    isAllCaps = false
                    maxLines = 1
                    gravity = Gravity.START or Gravity.CENTER_VERTICAL
                    layoutParams = LinearLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.WRAP_CONTENT,
                    )
                    setOnClickListener {
                        dialog.dismiss()
                        choice.onClick()
                    }
                },
            )
        }
        dialog.setContentView(root)
    }

    fun show() = dialog.show()

    private fun dp(context: Context, value: Int): Int =
        (value * context.resources.displayMetrics.density).toInt()
}

package dev.proflyder.home.core.presentation

import android.content.DialogInterface
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.annotation.DrawableRes
import androidx.core.view.isVisible
import androidx.fragment.app.FragmentManager
import com.google.android.material.bottomsheet.BottomSheetDialogFragment
import dev.proflyder.home.core.designsystem.R
import kotlinx.serialization.Serializable

data class SheetOption(
    val key: String,
    val label: CharSequence,
    @DrawableRes val icon: Int = 0,
)

@Serializable
data class SheetChoice(val key: String? = null)

open class OptionsResult : ResultKey<SheetChoice>(SheetChoice.serializer())

private object OptionsCodec : OptionsResult()

class OptionsSheet : BottomSheetDialogFragment() {
    private val args: Bundle get() = requireArguments()
    private val requestKey: String get() = args.getString(ARG_REQUEST).orEmpty()

    override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): View =
        inflater.inflate(R.layout.ds_sheet_options, container, false)

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        view.findViewById<TextView>(R.id.ds_sheet_title).text = args.getCharSequence(ARG_TITLE)
        val list = view.findViewById<LinearLayout>(R.id.ds_sheet_options)
        optionsOf(args).forEach { option -> list.addView(row(list, option, option.key == args.getString(ARG_SELECTED))) }
    }

    override fun onCancel(dialog: DialogInterface) {
        super.onCancel(dialog)
        parentFragmentManager.setFragmentResult(requestKey, OptionsCodec.pack(SheetChoice()))
    }

    private fun row(parent: ViewGroup, option: SheetOption, selected: Boolean): View {
        val row = layoutInflater.inflate(R.layout.ds_sheet_option_row, parent, false)
        val icon = row.findViewById<ImageView>(R.id.ds_sheet_option_icon)
        icon.isVisible = option.icon != 0
        if (option.icon != 0) icon.setImageResource(option.icon)
        row.findViewById<TextView>(R.id.ds_sheet_option_label).text = option.label
        row.findViewById<View>(R.id.ds_sheet_option_check).isVisible = selected
        row.isSelected = selected
        row.setOnClickListener { choose(option.key) }
        return row
    }

    private fun choose(key: String) {
        parentFragmentManager.setFragmentResult(requestKey, OptionsCodec.pack(SheetChoice(key)))
        dismiss()
    }

    companion object {
        private const val ARG_REQUEST = "ds_options_request"
        private const val ARG_TITLE = "ds_options_title"
        private const val ARG_KEYS = "ds_options_keys"
        private const val ARG_LABELS = "ds_options_labels"
        private const val ARG_ICONS = "ds_options_icons"
        private const val ARG_SELECTED = "ds_options_selected"

        fun show(
            manager: FragmentManager,
            result: OptionsResult,
            title: CharSequence,
            options: List<SheetOption>,
            selectedKey: String? = null,
        ) {
            val requestKey = result.requestKey
            if (manager.findFragmentByTag(requestKey) != null) return
            OptionsSheet().apply { arguments = argumentsOf(requestKey, title, options, selectedKey) }.show(manager, requestKey)
        }

        fun dismiss(manager: FragmentManager, result: OptionsResult) {
            (manager.findFragmentByTag(result.requestKey) as? OptionsSheet)?.dismissAllowingStateLoss()
        }

        private fun argumentsOf(requestKey: String, title: CharSequence, options: List<SheetOption>, selectedKey: String?) =
            Bundle().apply {
                putString(ARG_REQUEST, requestKey)
                putCharSequence(ARG_TITLE, title)
                putStringArray(ARG_KEYS, options.map(SheetOption::key).toTypedArray())
                putCharSequenceArray(ARG_LABELS, options.map(SheetOption::label).toTypedArray())
                putIntArray(ARG_ICONS, options.map(SheetOption::icon).toIntArray())
                putString(ARG_SELECTED, selectedKey)
            }

        internal fun optionsOf(args: Bundle): List<SheetOption> {
            val keys = args.getStringArray(ARG_KEYS).orEmpty()
            val labels = args.getCharSequenceArray(ARG_LABELS).orEmpty()
            val icons = args.getIntArray(ARG_ICONS) ?: IntArray(keys.size)
            return keys.indices.map { SheetOption(keys[it], labels[it], icons[it]) }
        }
    }
}

package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.fragment.app.viewModels
import androidx.lifecycle.SavedStateViewModelFactory
import com.google.android.material.bottomsheet.BottomSheetDialogFragment
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.SheetAlarmRepeatBinding

/**
 * Modal repeat editor: one list of presets plus an optional week of round day
 * toggles, replacing the cramped chips that used to live in the alarm editor.
 * The choice is applied only on "Done"; the editor receives it through the
 * Fragment Result API. The selected option and days are mirrored into the
 * ViewModel's [androidx.lifecycle.SavedStateHandle], so a rotation keeps them.
 */
internal class AlarmRepeatSheet : BottomSheetDialogFragment() {
    private var sheetBinding: SheetAlarmRepeatBinding? = null

    private val binding: SheetAlarmRepeatBinding
        get() = checkNotNull(sheetBinding) { "AlarmRepeatSheet binding outside its view lifecycle" }

    private val viewModel: AlarmRepeatSheetViewModel by viewModels {
        SavedStateViewModelFactory(requireActivity().application, this, arguments)
    }

    private var rendering = false

    /** Round day toggles in ISO order: index 0 is Monday (day 1). */
    private val dayToggles: List<MaterialButton>
        get() = with(binding) {
            listOf(repeatDay1, repeatDay2, repeatDay3, repeatDay4, repeatDay5, repeatDay6, repeatDay7)
        }

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?,
    ): View = SheetAlarmRepeatBinding.inflate(inflater, container, false).also { sheetBinding = it }.root

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        with(binding) {
            sheetCancel.setOnClickListener { dismiss() }
            sheetDone.setOnClickListener { publishResult() }
            repeatOptions.setOnCheckedChangeListener { _, checkedId ->
                if (!rendering) choose(checkedId)
            }
            dayToggles.forEachIndexed { index, toggle ->
                toggle.setOnClickListener { setDay(index + 1) }
            }
        }
        render()
    }

    override fun onDestroyView() {
        sheetBinding = null
        super.onDestroyView()
    }

    private fun render() = with(binding) {
        rendering = true
        repeatOptions.check(optionId(viewModel.choice.repeat))
        repeatDaysGroup.setVisible(viewModel.choice.repeat == RepeatKind.CUSTOM)
        dayToggles.forEachIndexed { index, toggle -> toggle.isChecked = viewModel.choice.days.contains(index + 1) }
        sheetDone.isEnabled = viewModel.canConfirm
        rendering = false
    }

    private fun choose(checkedId: Int) {
        viewModel.choose(repeatFor(checkedId))
        render()
    }

    private fun setDay(day: Int) {
        viewModel.setDay(day, !viewModel.choice.days.contains(day))
        render()
    }

    private fun publishResult() {
        val choice = viewModel.choice.normalized()
        parentFragmentManager.setFragmentResult(
            REQUEST_KEY,
            Bundle().apply {
                putString(RESULT_REPEAT, choice.repeat.name)
                putIntArray(RESULT_DAYS, choice.days.sorted().toIntArray())
            },
        )
        dismiss()
    }

    private fun repeatFor(checkedId: Int): RepeatKind = when (checkedId) {
        R.id.repeat_option_once -> RepeatKind.ONCE
        R.id.repeat_option_daily -> RepeatKind.DAILY
        R.id.repeat_option_weekdays -> RepeatKind.WEEKDAYS
        R.id.repeat_option_weekends -> RepeatKind.WEEKENDS
        else -> RepeatKind.CUSTOM
    }

    private fun optionId(repeat: RepeatKind): Int = when (repeat) {
        RepeatKind.ONCE -> R.id.repeat_option_once
        RepeatKind.DAILY -> R.id.repeat_option_daily
        RepeatKind.WEEKDAYS -> R.id.repeat_option_weekdays
        RepeatKind.WEEKENDS -> R.id.repeat_option_weekends
        else -> R.id.repeat_option_custom
    }

    companion object {
        const val TAG = "alarm_repeat_sheet"
        const val REQUEST_KEY = "alarm_repeat_request"
        const val RESULT_REPEAT = "alarm_repeat_result"
        const val RESULT_DAYS = "alarm_repeat_days_result"

        private const val ARG_REPEAT = AlarmRepeatSheetViewModel.KEY_REPEAT
        private const val ARG_DAYS = AlarmRepeatSheetViewModel.KEY_DAYS

        fun create(repeat: RepeatKind, days: Set<Int>): AlarmRepeatSheet = AlarmRepeatSheet().apply {
            arguments = Bundle().apply {
                putString(ARG_REPEAT, repeat.name)
                putIntArray(ARG_DAYS, days.sorted().toIntArray())
            }
        }
    }
}

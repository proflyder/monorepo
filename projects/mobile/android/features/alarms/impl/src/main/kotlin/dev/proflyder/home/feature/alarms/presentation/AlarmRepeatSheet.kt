package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.SavedStateViewModelFactory
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.bottomsheet.BottomSheetDialogFragment
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.presentation.CheckableBinder
import dev.proflyder.home.core.presentation.RadioGroupBinder
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.SheetAlarmRepeatBinding
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

internal class AlarmRepeatSheet : BottomSheetDialogFragment() {
    private var sheetBinding: SheetAlarmRepeatBinding? = null
    private var options: RadioGroupBinder<RepeatKind>? = null
    private var days: List<CheckableBinder<*>> = emptyList()

    private val viewModel: AlarmRepeatSheetViewModel by viewModels {
        SavedStateViewModelFactory(requireActivity().application, this, arguments)
    }

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?,
    ): View = SheetAlarmRepeatBinding.inflate(inflater, container, false).also { sheetBinding = it }.root

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        val binding = checkNotNull(sheetBinding)
        binding.sheetCancel.setOnClickListener { dismiss() }
        binding.sheetDone.setOnClickListener { viewModel.dispatch(AlarmRepeatIntent.Confirm) }
        options = RadioGroupBinder(binding.repeatOptions, ::optionId, ::repeatFor) { repeat ->
            viewModel.dispatch(AlarmRepeatIntent.Choose(repeat))
        }
        days = dayToggles(binding).mapIndexed { index, toggle ->
            CheckableBinder(toggle) { selected -> viewModel.dispatch(AlarmRepeatIntent.SetDay(index + 1, selected)) }
        }
        viewModel.state.observe { render(it) }
        viewModel.events.observe { onEvent(it) }
    }

    override fun onDestroyView() {
        sheetBinding = null
        options = null
        days = emptyList()
        super.onDestroyView()
    }

    private fun render(state: AlarmRepeatSheetState) {
        val binding = sheetBinding ?: return
        options?.render(state.choice.repeat)
        binding.repeatDaysGroup.setVisible(state.choice.repeat == RepeatKind.CUSTOM)
        days.forEachIndexed { index, day -> day.render(state.choice.days.contains(index + 1)) }
        binding.sheetDone.isEnabled = state.canConfirm
    }

    private fun onEvent(event: AlarmRepeatEvent) {
        when (event) {
            is AlarmRepeatEvent.Confirmed -> publish(event.choice)
        }
    }

    private fun publish(choice: AlarmRepeatChoice) {
        parentFragmentManager.setFragmentResult(
            REQUEST_KEY,
            Bundle().apply {
                putString(RESULT_REPEAT, choice.repeat.name)
                putIntArray(RESULT_DAYS, choice.days.sorted().toIntArray())
            },
        )
        dismiss()
    }

    private fun <T> Flow<T>.observe(action: (T) -> Unit) {
        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) { collect { action(it) } }
        }
    }

    private fun dayToggles(binding: SheetAlarmRepeatBinding) = with(binding) {
        listOf(repeatDay1, repeatDay2, repeatDay3, repeatDay4, repeatDay5, repeatDay6, repeatDay7)
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
        private const val RESULT_REPEAT = "alarm_repeat_result"
        private const val RESULT_DAYS = "alarm_repeat_days_result"

        fun create(repeat: RepeatKind, days: Set<Int>): AlarmRepeatSheet = AlarmRepeatSheet().apply {
            arguments = Bundle().apply {
                putString(AlarmRepeatSheetViewModel.KEY_REPEAT, repeat.name)
                putIntArray(AlarmRepeatSheetViewModel.KEY_DAYS, days.sorted().toIntArray())
            }
        }

        fun resultOf(result: Bundle): AlarmEditorIntent? {
            val repeat = RepeatKind.entries.firstOrNull { it.name == result.getString(RESULT_REPEAT) } ?: return null
            val days = result.getIntArray(RESULT_DAYS)?.toSet().orEmpty()
            return AlarmEditorIntent.SetRepeat(repeat, days)
        }
    }
}

package dev.proflyder.home.feature.timers.presentation

import dev.proflyder.home.core.presentation.bindText
import android.os.Bundle
import androidx.recyclerview.widget.LinearLayoutManager
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.TextBinder
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.injectedActivityViewModels
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.timers.TIMER_PRESETS
import dev.proflyder.home.feature.timers.impl.R
import dev.proflyder.home.feature.timers.impl.databinding.FragmentTimersBinding

internal class TimersFragment :
    ScreenFragment<FragmentTimersBinding, TimersUiState, TimersIntent, TimersEvent>(FragmentTimersBinding.Companion::inflate) {
    override val viewModel: TimersViewModel by injectedActivityViewModels()

    private val adapter = TimerAdapter(TimerAdapterCallback(::dispatch))
    private var label: TextBinder? = null
    private var duration: TextBinder? = null

    override fun onScreenCreated(binding: FragmentTimersBinding, savedInstanceState: Bundle?) {
        binding.timersContent.applyScrollBottomInset()
        binding.timersContent.keepFocusedVisible()
        binding.timersList.layoutManager = LinearLayoutManager(requireContext())
        binding.timersList.adapter = adapter
        label = bindText(binding.timersLabelInput, TimersIntent::SetLabel)
        duration = bindText(binding.timersDurationInput, TimersIntent::SetDuration)
        val chips = listOf(binding.timersPreset1, binding.timersPreset5, binding.timersPreset10, binding.timersPreset25)
        chips.zip(TIMER_PRESETS).forEach { (chip, preset) ->
            chip.setOnClickListener { dispatch(TimersIntent.Preset(preset)) }
        }
        binding.timersStart.setOnClickListener { dispatch(TimersIntent.Start) }
        binding.timersRefresh.setupPullToRefresh { dispatch(TimersIntent.Refresh(RefreshMode.HARD)) }
        binding.timersRetry.setOnClickListener { dispatch(TimersIntent.Refresh(RefreshMode.HARD)) }
    }

    override fun onBindingDestroyed() {
        binding.timersList.adapter = null
        label = null
        duration = null
    }

    override fun render(state: TimersUiState) = with(binding) {
        label?.render(state.form.label)
        duration?.render(state.form.duration)
        timersInputError.setVisible(state.form.durationInvalid)
        timersInputError.setText(R.string.timers_duration_error)
        val hasItems = state.timers.isNotEmpty()
        val failure = state.load.failure.takeIf { !hasItems }
        timersRefresh.isRefreshing = state.load.refreshing && hasItems
        timersProgress.setVisible(state.starting || (state.load.refreshing && !hasItems))
        timersStart.isEnabled = !state.starting
        timersErrorText.setVisible(failure != null)
        timersErrorText.text = failure?.userMessage(requireContext())
        timersRetry.setVisible(failure != null)
        timersEmpty.setVisible(!state.load.refreshing && failure == null && !hasItems)
        adapter.submitList(state.rows)
    }

    override fun onEvent(event: TimersEvent) {
        when (event) {
            is TimersEvent.Failure -> showFailure(event.failure)
            is TimersEvent.ConfirmCancel -> confirm(R.string.timers_cancel_confirm, R.string.timers_cancel_yes, R.string.timers_keep) {
                dispatch(TimersIntent.Cancel(event.id))
            }
        }
    }
}

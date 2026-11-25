package dev.proflyder.home.feature.timers.presentation

import dev.proflyder.home.core.presentation.bindText
import android.os.Bundle
import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.tabViewModels
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.timers.TIMER_PRESETS
import dev.proflyder.home.feature.timers.impl.R
import dev.proflyder.home.feature.timers.impl.databinding.FragmentTimersBinding

internal class TimersFragment :
    ScreenFragment<FragmentTimersBinding, TimersUiState, TimersIntent, TimersEvent>(FragmentTimersBinding.Companion::inflate) {
    override val viewModel: TimersViewModel by tabViewModels()

    private val adapter = TimerAdapter(TimerAdapterCallback(::dispatch))
    override fun onViewReady(binding: FragmentTimersBinding, savedInstanceState: Bundle?) {
        binding.timersContent.keepFocusedVisible()
        binding.timersList.adapter = adapter
        val label = bindText(binding.timersLabelInput, TimersIntent::SetLabel)
        val duration = bindText(binding.timersDurationInput, TimersIntent::SetDuration)
        val chips = listOf(binding.timersPreset1, binding.timersPreset5, binding.timersPreset10, binding.timersPreset25)
        chips.zip(TIMER_PRESETS).forEach { (chip, preset) ->
            chip.setOnClickListener { dispatch(TimersIntent.Preset(preset)) }
        }
        binding.timersStart.setOnClickListener { dispatch(TimersIntent.Start) }
        binding.timersRefresh.setupPullToRefresh(binding.timersContent) { dispatch(TimersIntent.Refresh(RefreshMode.HARD)) }
        binding.timersRetry.setOnClickListener { dispatch(TimersIntent.Refresh(RefreshMode.HARD)) }
        render({ it.form.label }, label::render)
        render({ it.form.duration }, duration::render)
        render({ it.form.durationInvalid }) { binding.timersInputError.setVisible(it) }
        render(::timersStatusOf, ::renderStatus)
        render(TimersUiState::rows, adapter::submitList)
    }

    override fun onBindingDestroyed() {
        binding.timersList.adapter = null
    }

    private fun renderStatus(status: TimersStatus) = with(binding) {
        val failure = status.load.failure.takeIf { !status.hasItems }
        timersRefresh.isRefreshing = status.load.refreshing && status.hasItems
        timersProgress.setVisible(status.starting || (status.load.refreshing && !status.hasItems))
        timersStart.isEnabled = !status.starting
        timersErrorText.setVisible(failure != null)
        timersErrorText.text = failure?.userMessage(requireContext())
        timersRetry.setVisible(failure != null)
        timersEmpty.setVisible(!status.load.refreshing && failure == null && !status.hasItems)
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

internal data class TimersStatus(val load: ListLoadState, val hasItems: Boolean, val starting: Boolean)

internal fun timersStatusOf(state: TimersUiState): TimersStatus =
    TimersStatus(state.load, state.timers.isNotEmpty(), state.starting)

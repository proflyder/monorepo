package dev.proflyder.home.feature.timers.presentation

import android.os.Bundle
import androidx.fragment.app.viewModels
import androidx.recyclerview.widget.LinearLayoutManager
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.timers.impl.R
import dev.proflyder.home.feature.timers.impl.databinding.FragmentTimersBinding
import kotlinx.coroutines.delay

class TimersFragment : BaseFragment<FragmentTimersBinding>(FragmentTimersBinding.Companion::inflate) {
    private val viewModel: TimersViewModel by viewModels {
        viewModelFactory(TimersViewModel::class.java)
    }

    private lateinit var adapter: TimerAdapter

    override fun onBindingCreated(binding: FragmentTimersBinding, savedInstanceState: Bundle?) {
        adapter = TimerAdapter(
            onPauseResume = viewModel::togglePause,
            onCancel = ::confirmCancel,
        )
        binding.timersList.layoutManager = LinearLayoutManager(requireContext())
        binding.timersList.adapter = adapter

        mapOf(
            binding.timersPreset1 to "1m",
            binding.timersPreset5 to "5m",
            binding.timersPreset10 to "10m",
            binding.timersPreset25 to "25m",
        ).forEach { (chip, duration) ->
            chip.setOnClickListener { binding.timersDurationInput.setText(duration) }
        }

        binding.timersStart.setOnClickListener { startTimer() }
        binding.timersRetry.setOnClickListener { viewModel.refresh() }

        viewModel.state.observe { render(it) }
        repeatOnView {
            while (true) {
                delay(1_000)
                viewModel.tick()
            }
        }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh()
    }

    private fun startTimer() {
        val duration = DurationText.parse(binding.timersDurationInput.text?.toString().orEmpty())
        if (duration !is DurationText.Result.Valid) {
            binding.timersInputError.setVisible(true)
            binding.timersInputError.text = getString(R.string.timers_duration_error)
            return
        }
        binding.timersInputError.setVisible(false)
        viewModel.start(binding.timersLabelInput.text?.toString().orEmpty(), duration.seconds)
    }

    private fun render(state: TimersUiState) {
        val hasItems = state.timers.isNotEmpty()
        val loadError = state.loadError
        val showLoadError = loadError != null && !hasItems
        binding.timersProgress.setVisible(state.starting || (state.refreshing && !hasItems))
        binding.timersStart.isEnabled = !state.starting
        binding.timersErrorText.setVisible(showLoadError)
        binding.timersErrorText.text = loadError?.userMessage(requireContext())
        binding.timersRetry.setVisible(showLoadError)
        binding.timersEmpty.setVisible(!state.refreshing && loadError == null && !hasItems)

        adapter.submitList(state.timers.map { TimerRow(it, viewModel.remaining(it)) })

        if (loadError != null && hasItems) {
            showFailure(loadError)
            viewModel.consumeLoadError()
        }
        state.actionError?.let { failure ->
            showFailure(failure)
            viewModel.consumeActionError()
        }
    }

    private fun confirmCancel(timer: Timer) {
        confirm(R.string.timers_cancel_confirm, R.string.timers_cancel_yes, R.string.timers_keep) {
            viewModel.cancel(timer)
        }
    }
}

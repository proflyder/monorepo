package dev.proflyder.home.feature.timers.presentation

import android.os.Bundle
import android.view.View
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.fragment.app.Fragment
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.button.MaterialButton
import com.google.android.material.snackbar.Snackbar
import com.google.android.material.textfield.TextInputEditText
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.timers.impl.R
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class TimersFragment : Fragment(R.layout.fragment_timers) {
    private val viewModel: TimersViewModel by viewModels {
        viewModelFactory(TimersViewModel::class.java)
    }

    private lateinit var adapter: TimerAdapter
    private lateinit var labelInput: TextInputEditText
    private lateinit var durationInput: TextInputEditText
    private lateinit var inputError: TextView
    private lateinit var startButton: MaterialButton
    private lateinit var progress: View
    private lateinit var errorText: TextView
    private lateinit var retry: MaterialButton
    private lateinit var empty: TextView

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        labelInput = view.findViewById(R.id.timers_label_input)
        durationInput = view.findViewById(R.id.timers_duration_input)
        inputError = view.findViewById(R.id.timers_input_error)
        startButton = view.findViewById(R.id.timers_start)
        progress = view.findViewById(R.id.timers_progress)
        errorText = view.findViewById(R.id.timers_error_text)
        retry = view.findViewById(R.id.timers_retry)
        empty = view.findViewById(R.id.timers_empty)

        adapter = TimerAdapter(
            onPauseResume = viewModel::togglePause,
            onCancel = ::confirmCancel,
        )
        val list = view.findViewById<RecyclerView>(R.id.timers_list)
        list.layoutManager = LinearLayoutManager(requireContext())
        list.adapter = adapter

        view.findViewById<View>(R.id.timers_preset_1).setOnClickListener { durationInput.setText("1m") }
        view.findViewById<View>(R.id.timers_preset_5).setOnClickListener { durationInput.setText("5m") }
        view.findViewById<View>(R.id.timers_preset_10).setOnClickListener { durationInput.setText("10m") }
        view.findViewById<View>(R.id.timers_preset_25).setOnClickListener { durationInput.setText("25m") }

        startButton.setOnClickListener { startTimer() }
        retry.setOnClickListener { viewModel.refresh() }

        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.state.collect(::render)
            }
        }
        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
                while (true) {
                    delay(1_000)
                    viewModel.tick()
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh()
    }

    private fun startTimer() {
        val duration = DurationText.parse(durationInput.text?.toString().orEmpty())
        if (duration !is DurationText.Result.Valid) {
            inputError.visibility = View.VISIBLE
            inputError.text = getString(R.string.timers_duration_error)
            return
        }
        inputError.visibility = View.GONE
        viewModel.start(labelInput.text?.toString().orEmpty(), duration.seconds)
    }

    private fun render(state: TimersUiState) {
        val hasItems = state.timers.isNotEmpty()
        progress.visibility = if (state.starting || (state.refreshing && !hasItems)) View.VISIBLE else View.GONE
        startButton.isEnabled = !state.starting
        errorText.visibility = if (state.loadError != null && !hasItems) View.VISIBLE else View.GONE
        errorText.text = state.loadError?.userMessage(requireContext())
        retry.visibility = if (state.loadError != null && !hasItems) View.VISIBLE else View.GONE
        empty.visibility = if (!state.refreshing && state.loadError == null && !hasItems) {
            View.VISIBLE
        } else {
            View.GONE
        }

        val rows = state.timers.map { TimerRow(it, viewModel.remaining(it)) }
        adapter.submitList(rows)

        if (state.loadError != null && hasItems) {
            Snackbar.make(
                requireView(),
                state.loadError.userMessage(requireContext()),
                Snackbar.LENGTH_LONG,
            ).show()
            viewModel.consumeLoadError()
        }
        state.actionError?.let { failure ->
            Snackbar.make(requireView(), failure.userMessage(requireContext()), Snackbar.LENGTH_LONG).show()
            viewModel.consumeActionError()
        }
    }

    private fun confirmCancel(timer: dev.proflyder.home.core.model.Timer) {
        AlertDialog.Builder(requireContext())
            .setMessage(R.string.timers_cancel_confirm)
            .setNegativeButton(R.string.timers_keep, null)
            .setPositiveButton(R.string.timers_cancel_yes) { _, _ -> viewModel.cancel(timer) }
            .show()
    }
}

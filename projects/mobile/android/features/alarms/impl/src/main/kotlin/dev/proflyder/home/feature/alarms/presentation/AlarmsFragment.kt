package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import android.view.View
import androidx.appcompat.app.AlertDialog
import androidx.fragment.app.Fragment
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.GridLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.floatingactionbutton.ExtendedFloatingActionButton
import com.google.android.material.snackbar.Snackbar
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.impl.R
import kotlinx.coroutines.launch

class AlarmsFragment : Fragment(R.layout.fragment_alarms) {
    private val viewModel: AlarmsViewModel by viewModels {
        viewModelFactory(AlarmsViewModel::class.java)
    }

    private lateinit var adapter: AlarmAdapter
    private lateinit var states: ListStateViews
    private lateinit var progress: View

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        adapter = AlarmAdapter(
            onClick = { alarm -> requireNavigator().navigate(AlarmEditorDestination(alarm.id)) },
            onToggle = { alarm, enabled -> viewModel.setEnabled(alarm, enabled) },
            onSnooze = viewModel::snooze,
            onStop = viewModel::stop,
            onDelete = { alarm -> confirmDelete(alarm.id) },
        )

        val list = view.findViewById<RecyclerView>(R.id.alarms_list)
        list.layoutManager = GridLayoutManager(requireContext(), resources.getInteger(R.integer.alarms_columns))
        list.adapter = adapter

        progress = view.findViewById(R.id.alarms_progress)
        states = ListStateViews(
            progress = progress,
            empty = view.findViewById(R.id.alarms_empty),
            error = view.findViewById(R.id.alarms_error_text),
            content = list,
        )
        // The error container must be hidden as a whole; ListStateViews only
        // controls the text view, so keep the container in sync.
        val errorContainer = view.findViewById<View>(R.id.alarms_error)

        view.findViewById<View>(R.id.alarms_retry).setOnClickListener { viewModel.refresh() }
        view.findViewById<ExtendedFloatingActionButton>(R.id.alarms_add).setOnClickListener {
            requireNavigator().navigate(AlarmEditorDestination())
        }

        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.state.collect { state ->
                    val hasItems = state.alarms.isNotEmpty()
                    when {
                        state.refreshing && !hasItems -> {
                            states.showLoading()
                            errorContainer.visibility = View.GONE
                        }
                        state.loadError != null && !hasItems -> {
                            states.showError(state.loadError.userMessage(requireContext()))
                            errorContainer.visibility = View.VISIBLE
                        }
                        !hasItems -> {
                            states.showEmpty(getString(R.string.alarms_empty))
                            errorContainer.visibility = View.GONE
                        }
                        else -> {
                            states.showContent()
                            errorContainer.visibility = View.GONE
                            adapter.submitList(state.alarms)
                            // Keep the existing list visible while a refresh is
                            // in flight; do not replace it with a spinner.
                            progress.visibility = if (state.refreshing) View.VISIBLE else View.GONE
                        }
                    }

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
                        // The switch may have moved optimistically; re-render
                        // the server-confirmed values.
                        adapter.notifyDataSetChanged()
                        viewModel.consumeActionError()
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh()
    }

    private fun confirmDelete(id: String) {
        AlertDialog.Builder(requireContext())
            .setMessage(R.string.alarms_delete_confirm)
            .setNegativeButton(R.string.alarms_cancel, null)
            .setPositiveButton(R.string.alarms_delete_yes) { _, _ ->
                viewModel.state.value.alarms.firstOrNull { it.id == id }?.let(viewModel::delete)
            }
            .show()
    }
}

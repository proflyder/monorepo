package dev.proflyder.home.feature.reminders.presentation

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
import dev.proflyder.home.feature.reminders.ReminderEditorDestination
import dev.proflyder.home.feature.reminders.impl.R
import kotlinx.coroutines.launch

class RemindersFragment : Fragment(R.layout.fragment_reminders) {
    private val viewModel: RemindersViewModel by viewModels {
        viewModelFactory(RemindersViewModel::class.java)
    }

    private lateinit var adapter: ReminderAdapter

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        adapter = ReminderAdapter(
            onClick = { reminder -> requireNavigator().navigate(ReminderEditorDestination(reminder.id)) },
            onToggle = viewModel::toggle,
            onDelete = { reminder -> confirmDelete(reminder.id) },
        )
        val list = view.findViewById<RecyclerView>(R.id.reminders_list)
        list.layoutManager = GridLayoutManager(requireContext(), resources.getInteger(R.integer.reminders_columns))
        list.adapter = adapter

        val states = ListStateViews(
            progress = view.findViewById(R.id.reminders_progress),
            empty = view.findViewById(R.id.reminders_empty),
            error = view.findViewById(R.id.reminders_error_text),
            content = list,
        )
        val errorContainer = view.findViewById<View>(R.id.reminders_error)
        view.findViewById<View>(R.id.reminders_retry).setOnClickListener { viewModel.refresh() }
        view.findViewById<ExtendedFloatingActionButton>(R.id.reminders_add).setOnClickListener {
            requireNavigator().navigate(ReminderEditorDestination())
        }

        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.state.collect { state ->
                    val hasItems = state.reminders.isNotEmpty()
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
                            states.showEmpty(getString(R.string.reminders_empty))
                            errorContainer.visibility = View.GONE
                        }
                        else -> {
                            states.showContent()
                            errorContainer.visibility = View.GONE
                            adapter.submitList(state.reminders)
                            view.findViewById<View>(R.id.reminders_progress).visibility =
                                if (state.refreshing) View.VISIBLE else View.GONE
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
            .setMessage(R.string.reminders_delete_confirm)
            .setNegativeButton(R.string.reminders_cancel, null)
            .setPositiveButton(R.string.reminders_delete_yes) { _, _ ->
                viewModel.state.value.reminders.firstOrNull { it.id == id }?.let(viewModel::delete)
            }
            .show()
    }
}

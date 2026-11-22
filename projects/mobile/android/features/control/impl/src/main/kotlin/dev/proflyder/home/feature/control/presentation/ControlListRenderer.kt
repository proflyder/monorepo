package dev.proflyder.home.feature.control.presentation

import androidx.recyclerview.widget.GridLayoutManager
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.ControlItems
import dev.proflyder.home.feature.control.ControlTexts
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding

internal class ControlListRenderer(
    private val binding: FragmentControlBinding,
    dispatch: (ControlIntent) -> Unit,
) {
    private val context = binding.root.context
    private val adapter = ControlAdapter(ControlAdapterCallback(dispatch))
    private val texts by lazy(::controlTexts)
    private var hubWasEmpty = true
    private val states = ListStateViews(
        progress = binding.controlProgress,
        empty = binding.controlEmpty,
        error = binding.controlError,
        content = binding.controlContent,
        errorText = binding.controlErrorText,
    )

    init {
        val spanCount = context.resources.getInteger(R.integer.control_columns)
        val deviceSpan = spanCount / context.resources.getInteger(R.integer.control_device_columns)
        binding.controlList.layoutManager = GridLayoutManager(context, spanCount).apply {
            spanSizeLookup = ControlSpans(adapter, spanCount, deviceSpan)
        }
        binding.controlList.adapter = adapter
    }

    fun render(state: ControlUiState) {
        val catalog = state.catalog
        val hasItems = state.entries.isNotEmpty()
        val failure = catalog.failure
        when {
            !catalog.configured -> showError(context.getString(R.string.control_not_configured), configured = false)
            catalog.loading && !hasItems -> states.showLoading()
            failure != null && !hasItems -> showError(failure.userMessage(context), configured = true)
            !hasItems -> states.showEmpty()
            else -> showContent(state)
        }
        binding.controlRefresh.isRefreshing = catalog.loading && hasItems
        binding.controlStatesBanner.setVisible(hasItems && !catalog.statesOk)
    }

    private fun showError(message: String, configured: Boolean) {
        states.showError(message)
        binding.controlRetry.setVisible(configured)
        binding.controlOpenConnection.setVisible(!configured)
    }

    private fun showContent(state: ControlUiState) {
        states.showContent()
        val stickToTop = hubWasEmpty && state.hub.cards.isNotEmpty() && !binding.controlList.canScrollVertically(-1)
        hubWasEmpty = state.hub.cards.isEmpty()
        adapter.submitList(items(state)) { if (stickToTop) binding.controlList.scrollToPosition(0) }
    }

    private fun items(state: ControlUiState): List<ControlItem> = ControlItems.build(
        entries = state.entries,
        pending = state.pending,
        statesOk = state.catalog.statesOk,
        filter = state.filter,
        texts = texts,
        expandedDevices = state.layout.expandedDevices,
        leading = listOf(ControlItem.Hub(state.hub.cards, state.hub.remaining, state.hub.nowUnix)),
        collapsedRooms = state.layout.collapsedRooms,
    )

    private fun controlTexts() = ControlTexts(
        otherDevices = context.getString(R.string.control_section_other),
        scenarios = context.getString(R.string.control_section_scenarios),
        unavailable = context.getString(R.string.control_section_unavailable),
        onSummary = { on, total -> context.getString(R.string.control_on_summary, on, total) },
        all = context.getString(R.string.control_filter_all),
        lights = context.getString(R.string.control_filter_lights),
        switches = context.getString(R.string.control_filter_switches),
    )
}

private class ControlSpans(
    private val adapter: ControlAdapter,
    private val spanCount: Int,
    private val deviceSpan: Int,
) : GridLayoutManager.SpanSizeLookup() {
    override fun getSpanSize(position: Int): Int = when (adapter.currentList.getOrNull(position)) {
        is ControlItem.Scenario -> 1
        is ControlItem.Device, is ControlItem.DeviceGroup -> deviceSpan
        else -> spanCount
    }
}

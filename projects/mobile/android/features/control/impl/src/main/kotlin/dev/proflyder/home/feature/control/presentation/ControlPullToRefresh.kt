package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding

internal fun wirePullToRefresh(binding: FragmentControlBinding, onRefresh: () -> Unit) {
    binding.controlRefresh.setupPullToRefresh(binding.controlList, onRefresh = onRefresh)
}

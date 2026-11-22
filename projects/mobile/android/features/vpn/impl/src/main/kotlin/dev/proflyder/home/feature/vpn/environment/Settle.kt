package dev.proflyder.home.feature.vpn.environment

import kotlin.time.Duration
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.withIndex

@OptIn(FlowPreview::class)
internal fun <T> Flow<T>.settle(window: Duration): Flow<T> = withIndex()
    .debounce { if (it.index == 0) Duration.ZERO else window }
    .map { it.value }

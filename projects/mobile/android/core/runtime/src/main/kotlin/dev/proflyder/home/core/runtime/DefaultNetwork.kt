package dev.proflyder.home.core.runtime

import kotlinx.coroutines.flow.StateFlow

interface DefaultNetwork {
    val handle: StateFlow<Long?>
}

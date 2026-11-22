package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.getAndUpdate
import kotlinx.coroutines.flow.update

internal class PendingActions {
    private val ids = MutableStateFlow<Set<String>>(emptySet())

    val pending: StateFlow<Set<String>> = ids.asStateFlow()

    suspend fun run(id: String, action: suspend () -> Outcome<*>): Outcome<*>? {
        if (id in ids.getAndUpdate { it + id }) return null
        return action().also { ids.update { it - id } }
    }
}

internal fun Outcome<*>?.report(onSuccess: () -> ControlEvent?): ControlEvent? = when (this) {
    null -> null
    is Outcome.Success -> onSuccess()
    is Outcome.Failure -> ControlEvent.Failure(failure)
}

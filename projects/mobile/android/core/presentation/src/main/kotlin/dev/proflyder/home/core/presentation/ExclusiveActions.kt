package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

class ExclusiveActions(
    private val scope: CoroutineScope,
    private val onBusy: (String?) -> Unit,
    private val onFailure: (RemoteFailure) -> Unit,
) {
    private var runningKey: String? = null

    val busy: Boolean get() = runningKey != null

    fun <T> run(key: String, action: suspend () -> Outcome<T>, onSuccess: (T) -> Unit) {
        runAll(key, listOf(Unit)) { _ -> action().settle(onSuccess) }
    }

    fun <T> runAll(key: String, targets: List<T>, step: suspend (T) -> RemoteFailure?) {
        if (busy || targets.isEmpty()) return
        mark(key)
        scope.launch {
            val failure = targets.firstNotNullOfOrNull { step(it) }
            mark(null)
            failure?.let(onFailure)
        }
    }

    private fun mark(key: String?) {
        runningKey = key
        onBusy(key)
    }
}

fun <T> Outcome<T>.settle(onSuccess: (T) -> Unit): RemoteFailure? = when (this) {
    is Outcome.Success -> {
        onSuccess(value)
        null
    }
    is Outcome.Failure -> failure
}

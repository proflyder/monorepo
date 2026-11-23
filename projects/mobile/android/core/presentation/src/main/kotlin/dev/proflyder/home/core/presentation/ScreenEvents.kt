package dev.proflyder.home.core.presentation

import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.lifecycle.withStateAtLeast
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

interface DeliveredAt {
    val minState: Lifecycle.State
}

interface DeliveredWhenStarted : DeliveredAt {
    override val minState: Lifecycle.State get() = Lifecycle.State.STARTED
}

val Any.deliveryState: Lifecycle.State
    get() = (this as? DeliveredAt)?.minState ?: Lifecycle.State.RESUMED

data class QueuedEvent<out E : Any>(val id: Long, val event: E)

class ScreenEvents<E : Any> {
    private val queue = MutableStateFlow<List<QueuedEvent<E>>>(emptyList())
    private var sequence = 0L

    val next: Flow<QueuedEvent<E>> = queue.mapNotNull { it.firstOrNull() }.distinctUntilChanged()

    val consumed: Flow<E> = next.onEach(::consume).map { it.event }

    fun emit(event: E) {
        sequence += 1
        val queued = QueuedEvent(sequence, event)
        queue.update { it + queued }
    }

    fun consume(item: QueuedEvent<E>) {
        queue.update { pending -> pending.filterNot { it.id == item.id } }
    }
}

fun <E : Any> LifecycleOwner.deliverEvents(events: ScreenEvents<E>, onEvent: (E) -> Unit) {
    lifecycleScope.launch {
        repeatOnLifecycle(Lifecycle.State.CREATED) {
            events.next.collect { item -> deliver(events, item, onEvent) }
        }
    }
}

private suspend fun <E : Any> LifecycleOwner.deliver(events: ScreenEvents<E>, item: QueuedEvent<E>, onEvent: (E) -> Unit) {
    lifecycle.withStateAtLeast(item.event.deliveryState) {
        events.consume(item)
        onEvent(item.event)
    }
}

package dev.proflyder.home.core.presentation

import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.filterIsInstance

interface AppEvent

interface AppEventBus {
    val events: Flow<AppEvent>

    fun publish(event: AppEvent)
}

inline fun <reified T : AppEvent> AppEventBus.eventsOf(): Flow<T> = events.filterIsInstance<T>()

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class SharedFlowAppEventBus : AppEventBus {
    private val stream = MutableSharedFlow<AppEvent>(
        extraBufferCapacity = EVENT_BUFFER,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    override val events: Flow<AppEvent> = stream.asSharedFlow()

    override fun publish(event: AppEvent) {
        stream.tryEmit(event)
    }

    private companion object {
        const val EVENT_BUFFER = 64
    }
}
